# Deliberate differences from the reference implementation

hydrus-rs matches the reference implementation's observable behaviour except
where listed here. Each entry says what differs, why, and how it is checked.
Add to this file in the same change that introduces a difference.

## Search predicate parsing (`hydrus-search`)

Checked by `crates/hydrus-search/tests/reference_parity.rs` against
`oracle/fixtures/system_predicates.json`: the first six differences are named
outcomes in that test, each verified specifically rather than just tolerated.
The last three are covered by the crate's unit tests.

- **Time values use a documented grammar instead of `dateparser`.** Dates are
  `YYYY-MM-DD` (or `/`, `.`) with an optional `HH:MM[:SS]`; ages are
  `<n> <unit>` terms with an optional `ago`, or `yesterday`. Natural language
  ("today", "last week", "4 march 2020"), day/month-first dates, fractions,
  number words, future times and the junk `dateparser` happens to accept are
  rejected. The test lists every such corpus input.
- **Ages stay ages.** The reference treats a value without the words
  year/month/day/hour/second/ago (e.g. `2 weeks`, `30 minutes`, `1d`) as a
  wall-clock date fixed at parse time, which inverts `<`/`>` ("imported
  before two weeks ago" for `< 2 weeks`). We keep it an age; the test checks
  that it denotes the same instant at the oracle's pinned "now". "The day
  of"/"the month of" an age is refused.
- **Ages keep calendar units and sub-hour precision.** The reference converts
  years and months to days at parse time and rounds to whole hours; we keep
  `CalendarDelta { years, months, days, hours, minutes, seconds }` and apply
  it when the search runs.
- **Hash lengths are checked.** `system:hash` and `system:similar to` reject a
  hash whose length does not fit its hash type (the reference accepts any
  length and matches nothing).
- **Case is preserved for URL regexes, exact URLs and note names**, which are
  matched case-sensitively; the reference lowercases them, so mixed-case
  values never match (its code to prevent this is unreachable).
- **Numbers must fit in 64 bits** (Python integers are unbounded).
- **An empty tag in `system:has tag` is an error** rather than a search for
  the literal tag `invalid tag`.
- **The Client API `tags` parameter must be a JSON list.** The reference
  iterates a string character by character.
- **Advanced tag statuses are a set**, so a status named twice is stored once.

## Search execution (`hydrus-search::exec`)

Checked by the conformance runner: the recorded steps are listed, with these
reasons, in `crates/hydrus-api/tests/known_differences.toml`, and the runner
fails if one starts matching.

- **Every sort the Client API accepts is applied.** The reference accepts
  sorting by number of tags and by has-audio, then returns the files
  unsorted (its database only applies a fixed list of "simple" sorts).
- **`system:tag as number` with `≈` needs one tag in range.** The reference
  tests "above the lower bound" and "below the upper bound" separately, so a
  file tagged `page:5` and `page:20` matches `page ≈ 10`.
- **"Deleted" in `system:has tag with status` means deleted** for display
  tags too; the reference searches pending tags instead.
- **A saved search's URL class predicate uses the client's class of that
  name as it is now** (found case-folded, as a typed search finds it); the
  reference tests the copy of the class saved with the search. If no class
  has that name any more, the search fails ("Did not find URL Class
  called ..."), where the reference would still use its copy.

Searches by URL class are checked by
`crates/hydrus-api/tests/url_class_search.rs` on
`oracle/fixtures/url_class_search.json`: URL classes on a domain with and
without its subdomains, a domain regex, query parameters in another order
and a name that only matches case-folded, searched through the Client API
and tested in memory.

## Media (`hydrus-media`)

- **Values from ffmpeg depend on your ffmpeg.** Video and audio (frames,
  durations) and PSDs are read through ffmpeg, as hydrus reads them, and
  different ffmpeg versions and builds give slightly different thumbnails
  and durations, for hydrus too. The parity tests compare these values
  with the ffmpeg the fixtures were recorded with (Ubuntu 24.04's 6.1, on
  x86-64); anywhere else they only report them.
- **AVIF, HEIF and JPEG XL images are decoded by ffmpeg** rather than the
  Python libraries hydrus uses, so their pixels can differ slightly, and
  what works depends on the ffmpeg: 6.1 can't read HEIF images at all
  (they get the default thumbnail) and drops AVIF transparency. Newer
  ffmpeg (9 at least) reads both, and gives an image's transparency as a
  separate stream, which we merge back. XMP inside AVIF and HEIF images
  isn't looked for yet, so those files don't get the XMP flag at import.

## File lifecycle (`hydrus-store::content`)

Checked by the property tests in `crates/hydrus-store/src/content/tests.rs`
(the umbrella domains always equal their definitions) and by the Client API
conformance runner, which skips exactly the recorded fields listed in
`crates/hydrus-api/tests/known_differences.toml`.

- **"Deleted from anywhere" is always the union of deletion records.**
  Deleting a file from a domain it isn't in (a pre-emptive delete, which
  blocks a future import) records the deletion; the reference then leaves the
  file out of its combined deleted domain until a full resync. We add it
  straight away.
- **File metadata reflects the database, not a stale cache.** The reference
  serves file metadata from an in-memory cache that some writes update
  incompletely until the next restart: purging a file from local storage
  archives it but the cache has no archive time, and clearing a deletion
  record leaves the cache reporting the combined-local-media deletion and
  "deleted from anywhere" membership that the database no longer has. We
  have no such cache and report what the reference reports after a restart.
- **Automatic maintenance has an explicit activity source.** GUI trash and
  deferred-delete passes sample the owned live idle monitor; the daemon has
  no GUI activity and is classified as normal time. Both consumers honor the
  saved normal-time flags (default on). Broader system-busy and scheduling
  parity remains separate from these finite gates.

## Duplicates (`hydrus-store::duplicates`)

Relationship bookkeeping is checked by the random `relationships_random_*`
conformance scenarios; metadata merges by `crates/hydrus-api/tests/duplicate_merges.rs`
against the reference's *database* after each decision
(`oracle/fixtures/duplicate_merges.json`), because its API answers after
merges come from the in-memory cache described below.

- **Several pairs in one request are decided in order.** The reference
  computes every pair's metadata merge in `set_file_relationships` from the
  files as they were before the request. A later pair can then undo an
  earlier one: seen with A better than B, then B better than C, where the
  first pair moves B's tags to A and the second doesn't re-add to B the tags
  C had in common with B's *old* tags, so those tags are lost from every
  file. We apply each pair's merge to the state the previous pairs left.
- **Metadata after a merge is read from the database.** After merges the
  reference's in-memory media cache can show deletion records its database
  doesn't hold (seen: a deleted parent tag on the better file) until a
  restart. We report the database, as it does after a restart.
- **`king_is_on_file_domain` and `king_is_local` describe the king.** The
  reference computes them from the file asked about, so a trashed file whose
  king isn't trashed reports its own state. We report the king's, as the
  Client API documentation says.
- **The potential pairs count is never stale.** The reference caches, per
  file domain, which potential pairs are visible; the cache misses pairs that
  come into view later (e.g. when a trashed king is replaced with one in the
  domain) until a restart. We count from the database.
- **A group whose king is out of view is shown by its lowest-id member.** When
  a duplicate group's king isn't in the searched domain, the reference shows
  a random member in its place; we show the member with the lowest file id,
  so answers are repeatable. (The oracle pins the reference's choice the same
  way when recording.)
- **Deleted and petitioned tags are shown as stored.** Not a difference, but
  easy to get wrong: siblings and parents only apply to a file's current and
  pending tags; its "display" deleted and petitioned tags are its storage ones.

## Similar files and potential duplicates

Checked by `crates/hydrus-import/tests/similar_files.rs` on
`oracle/fixtures/similar_files.json` (the reference searching generated
near-duplicates at distance 2 and then 4) and by the fixture database's own
search.

- **No tree in the database.** The reference keeps a VP-tree of perceptual
  hashes in its database and rebalances it in the background; we index the
  hashes in memory when a search runs. Which hashes are within a distance
  doesn't depend on the index, so the same pairs are found.
- **The search runs whenever it has work, if either of the reference's
  "search during active/idle time" options is on.** Without a GUI there is
  no idle time to wait for; the search is fast enough not to need it.
- **When one search finds two files of the same duplicate group at different
  distances, the pair gets the smaller.** The reference records whichever
  its tree walk reached first.

## The client and the daemon (`hydrus-gui`, `hydrus serve`)

- **The work runs in a separate process.** Downloads, subscriptions,
  import and export folders, maintenance and the Client API run in `hydrus
  serve`, which the client starts while none runs (DECISIONS.md,
  2026-10-01). So a daemon started on its own runs on after the client
  closes; the client's own one gets 20 seconds to finish what it is doing
  before it is killed (the reference waits for its jobs as it closes).
  `crates/hydrus-gui/src/daemon.rs` and `crates/hydrus-cli/tests/serve.rs`
  check the starting and stopping.
- **`/manage_pages` answers from the store**, where the client keeps its
  pages (the last session, the page shown and each page's files and
  selection) as they change, within half a second; what the endpoints ask
  of a page (focusing it, adding files, refreshing it) the client does the
  next time it looks, after the answer. With the client closed they answer
  from the session it will open with: added files join it, a focused page
  is the one it opens on, and refreshing waits until the page is open
  again. The recorded `pages` scenario (`oracle/recordings/pages.json`)
  replays in `crates/hydrus-api/tests/conformance.rs` (with the client
  closed), and `crates/hydrus-api/tests/pages.rs` and the session test in
  `crates/hydrus-gui/tests/gui/session.rs` cover the rest.
  `get_page_info` describes a URL, gallery, watcher or local import
  page's importers as
  the reference does, but each search's and watcher's key
  (`gallery_key`, `watcher_key`, `highlight`) is its queue's number
  written as 64 hex digits, where the reference makes random ones.
- **Page keys last.** The reference makes new keys for its pages (and its
  top notebook) each time it starts; ours are kept with the session, so a
  tool can keep one.
- **Refreshing a page of pages refreshes the pages in it.** The
  reference fails with a server error (notebooks have no `RefreshQuery`).
- **The client opens on the page it showed last**, or the one
  `focus_page` asked for while it was closed. The reference opens each
  notebook on its first page.
- **A page's selected files are listed in the page's order**
  (`hash_ids_selected`); the reference's order is its selection set's.
- **A page's state is always "normal"** (`page_state` 0): our searches
  finish before the page is shown again, so it is never seen searching.
- **One client at a time on a store**, as the reference allows one client
  on its database; a second says the store is already open.

## Pages (`hydrus-gui`)

- **Active predicate editing remains Partial.** Represented system values and
  simple tags/namespaces/wildcards can now be edited together, with one atomic
  Apply and separate supplied values for repeated system families. Immutable
  terms are preserved and invertible terms have flip buttons. Active-list
  remove/invert/OR merge or dissolve/namespace commands reach the query. Populated
  OR controls embedded in a mixed edit, inherited sibling/parent editors and
  their asynchronous relationship information, favourite/most-used tags and
  maintenance branches remain missing, so the original action earns no completion
  credit. Inherited copy/open transports now work, including collapsed OR copying
  and real AND/OR/per-predicate/duplicate-filter pages. Per-predicate pages open
  in native list order; Qt iterates its selected-term set. Qt’s active-list file
  selection handler is an inherited no-op, distinct from its media-list handler.
  Single system editors keep their existing per-panel OK/star topology. Selection
  is cleared when query terms change, whereas Qt selects newly edited terms. A
  page transition cancels the child rather than retaining a modal editor behind
  a switched page. Remaining list keyboard navigation is unchanged.
- **Favourite search autocomplete shares the native page's suggestion model**,
  including blank system predicate editors, and has editable file/tag domains,
  current/pending tags, sort and collection controls. It does not yet offer the
  reference's autocomplete cog, explicit OR-construction buttons, favourite
  predicates or fetch/children tabs. Multiple/deleted location selection and
  overwrite/delete questions appear inside the owner window. The list sorts
  names casefolded as lowercase (Python's casefold differs for a few letters,
  such as "ß", which we fold to "ss" as it does).

- **Dimensions preset verification** separately tracks nine historical first-pass
  controls. The new Qt recording activates actual buttons through the real modal
  consumer, records ordered outputs/live recent history and cancellation, and
  captures the dialog. Qt activation is a control signal, not physical pointer
  input. Native regressions use measured pointer targets and check durable recent
  history, cancellation and retired-owner guards. Page/history comparisons prove
  membership, not ordering parity. Guard tests invoke callbacks; only preset
  activation dispatches physical pointer events. Editable dimension operators,
  tolerance/operand fields and broader layout remain outside this slice; the
  existing overlap in editable width/height unit text is not promoted by preset
  verification. Full Linux execution and independent native render review passed
  for exactly nine historical leaves, with zero additional implementation credit.
  Evidence (checkpoint `dimensions-e00ddab70`, in git history before 2026-10-08).

- **System predicate editors type dates** ("2011-06-04", and "13:05") where
  the reference's have a calendar and a time box. Viewing-time predicates
  now preserve the millisecond fields through creation, reference import,
  recent predicates and database searches, including the reference's
  floating-point truncation at the query boundary. The free-text parser
  continues to accept whole-second viewtime intervals. Legacy viewtimes
  with submillisecond precision round to the nearest millisecond; exact
  arbitrary-float import parity remains unfinished. A file size in
  terabytes, which the reference's editor offers
  but can't write out ("error:cannot render this predicate"), is written
  "200TB"; neither parser takes "TB". File-size comparisons now use five
  radio choices; hash has its vertical is/is-not and four-type radio groups;
  the other predicates' radio buttons remain drop-downs
  (as are the like/dislike and star controls of "system:rating"), and an
  editor's rows don't wrap: a narrow window scrolls sideways. A filetype
  group partly ticked shows unticked (Qt's tree shows it part-ticked).
  Hash cleanup and acceptance warnings now use an owned Warning/OK notice;
  other predicates' warnings remain under the panels. Hash forced cleanup already
  asked "You sure?"; that question remains inline in the owner, with edits and
  acceptance blocked until answered. Native typed hash sets reconstruct lines in
  deterministic hash order; Qt can retain the order of an explicitly supplied
  tuple. Both preserve the selected algorithm, sign and query set. Generic active
  predicate Edit remains a separate scope; this hash slice exercises explicit
  values in the model and saved-default reopening through the native editor.
  The global Enter-on-radio preference remains outside this slice. Native warning
  styling uses the existing Slint notice rather than Qt's warning icon. Hash
  source regressions and PNG captures are authored; hosted execution/rendered
  inspection are pending.
  "Paste image!" takes a file's path from the clipboard, not image data.
  A recent predicate is forgotten with a "forget" button where the
  reference has a trash icon. The star menu now saves/resets typed defaults
  immediately, surviving owner Cancel and keeping current fields unchanged on
  reset. Star Save can retain an invalid regex, as the reference's Save path
  bypasses its separate acceptance check; OK still reports the invalid regex.
  Its date/relative, views/viewtime, URL-type and cross-service rating
  comparability follows the actual reference. Per-service rating panels preserve
  the reference's omission of custom-default initialization; advanced rating uses
  it. Legacy defaults are imported into the same canonical typed store setting;
  unreadable future records stay lossless in imported options and inactive in
  editors. URL class defaults
  retain the existing native predicate's class-name identity. Date and fractional
  viewtime precision retain the limits above.

- **Still resampling under thread exhaustion** leaves the original image
  visible with Slint's nearest-pixel scaling when the OS cannot start its
  worker. It does not block the UI or poll for an unavailable result; a changed
  file, zoom or clipping plan can retry. Normal resampling remains unchanged.

- **Thumbnails on a scaled screen are resampled to its pixels** (area
  when shrinking, Lanczos when growing, as the reference resizes
  thumbnails), where the reference has Qt scale them as it draws. Slint's
  software renderer scales images by picking the nearest pixels, which
  made them blocky, so we never let it scale one. A selected thumbnail's
  border is at least 2 pixels wide (in the accent colour), so a selection
  shows with a thin or no border; the reference's is the border's width,
  with the cell in its selected colour.

- **A numerical rating's "3/5" over a thumbnail is measured in our font,
  roughly**: the reference sizes its box and places the stars after it by
  Qt's measure of the text in its font. We estimate the text's width
  (each character six tenths of the font's size), so a box with a "3/5"
  in it may be a pixel or two wider or narrower than the reference's;
  everything else in the ratings' layout is the reference's to the
  pixel.

- **The status bar has the page's and the network's parts only**: the
  reference's idle, busy-threads, CPU-busy and database parts aren't
  there (the daemon is never idle, and its threads and database aren't
  the client's). The network part counts what the daemon reads, which
  may run without the client; what it read before the client opened
  isn't counted, as the reference's session starts with it. The main
  window is titled "hydrus-rs", where the reference's is "main" with Qt's
  "hydrus client 688" after it.

- **The menu bar is drawn by hydrus-rs** (Slint's own can't be built from
  a list of entries), so it is the same on every system: macOS's isn't at
  the top of the screen, and menus stay inside the window. Hydrus's entries
  that hydrus-rs can't do yet are greyed out rather than left out, and a
  greyed tick box shows unticked whatever hydrus had. Left out: help >
  debug (hydrus's own debugging tools) and "about Qt"; the services menu's
  "administrate", for repository admins; the database menu's backup
  entries as hydrus has them for a database across several locations;
  and the
  undo menu's content undo and redo. Search predicate history now has native
  addition/removal consumers and confirmed clearing, checked against an
  executed actual Qt recording; its authored model/native replay awaits CI.
  Changes within one batch are kept in deterministic predicate order, whereas
  Qt emits a Python set difference whose internal order can vary between
  processes. Populated locked pages retain their badge and media after
  hidden-query Undo, including with synchronization enabled, as recorded
  from the reference.
  Hydrus's menu entries describe themselves in the status bar as the
  pointer passes; ours don't yet, and the history's latest page isn't in
  bold. Saving a session asks its name and its questions in one dialog,
  and says a name can't be had over the name box, where the reference
  shows a message box first; and "clear and load" isn't there yet.
- **The about window describes hydrus-rs**: its name, its own version
  beside the hydrus version it ports ("v0.1.0, porting hydrus v688, using
  network version 20"), and, on its description tab, the reference's
  lines that mean something for it (the platform, ffmpeg and SQLite
  versions, the boot time, the directories, and the store's cache size,
  journal and synchronous modes, as SQLite reports them); Python's
  libraries, Qt, the locale, the commit period and temp-in-memory lines
  aren't there, and the optional libraries tab lists ffmpeg alone. The
  boot time is in UTC, and there is no hydrus icon over the name.
- **The options window has only the options hydrus-rs honours** (so far
  those on twenty-five pages; the others, and pages with none, aren't there:
  on the connection page, the CA bundle and curl_cffi test; on the
  downloading page, the default download source, the
  number of subscriptions syncing at once and the failed-imports limit; on
  the maintenance page, idle time and shutdown, repository, sibling,
  deferred delete and idle work settings; on the duplicates page, the
  filter's colours; on the file
  viewing statistics page, the filters' own switches and the menus'
  stats; most of the gui page; on the importing page, dropped URLs and
  the work slots; and on the media playback page, the preview's zoom,
  re-centring, the checkerboard, animations, mpv, Qt's player and the
  system settings; the system page omits filesystem wake waiting, and the GUI
  has no periodic sleep checker of its own (the downloader daemon does); on the tag sort page,
  the namespace grouping list; on the ratings page, the example
  rating service's dropdown, the clickable examples, and the preview
  window's and dialogs' sizes; and on the thumbnails page, exact whole-bitmap fading
  and the rendering tech). Options' tooltips aren't shown;
  a box's title is a heading over its options rather than a frame around
  them; a sort's type is a dropdown of the types a page's sort control
  lists, where the reference's is a button opening a menu of them, and a
  collect's choices are checkboxes under its label, with its unmatched
  files' choice, where the reference's are a dropdown and a cog menu; and a time behind a button in the reference (the downloaders'
  waits after errors) shows its fields in place. Its search suggests only
  the options it has, their boxes, auxiliary labels and initial dropdown values;
  broader explanatory text on absent pages is unavailable. Application naming
  reaches the main-window title; secondary window titles still use their existing
  captions. Regex favourites open in a child list editor rather than embedding
  that list on the options page; their changes still wait for the parent Apply.
  Exit confirmation honors the switch and auto-accept timeout; importer
  activity reasons and shutdown-maintenance questions are not yet included. Search position and
  remembering the last panel are editable. Connection/error-delay ranges follow the saved advanced mode
  when opening the window; changing that mode takes effect on reopening, as in
  the reference. Two options with the same label each go to their own
  row (the reference's both go to the last). The checker options editor
  has no help button, and raises a time below its least value to it when
  "apply" is pressed, where the reference's does as the focus leaves the
  time (the same, as pressing "apply" takes the focus).

- **The page chooser takes the top row's digits too.** The reference takes
  only the number pad's; Slint doesn't tell them apart.
- **A closed URL downloader page's downloads wait** until it is reopened
  (Ctrl+U), and are deleted with it after the hour or when the client
  closes (or, after a crash, when it next opens). The reference's closed
  page imports on out of sight until it is destroyed; ours stops, as the
  close question's "This page is still importing." suggests, and a daemon
  left running without the client doesn't work on a page nobody can see.
  `crates/hydrus-cli/tests/serve.rs` and `crates/hydrus-gui/tests/gui/session.rs`
  check it.
- **A download's progress reaches its page within about three quarters
  of a second**: the daemon keeps what its queues are doing in the store
  four times a second, and the page looks twice a second. The reference's
  page reads its importer directly. The line itself is the reference's
  (`NetworkJobControl`, recorded for 337 jobs in
  `oracle/fixtures/import_status.json`), less its cog and error menus.
- **A URL downloader page shows every file its queue imported or found
  already in the database**, the reference's default presentation; a
  page's own presentation options (new files only, say) aren't applied
  yet, and the page has no sort or collect controls yet: its files are in
  the queue's order.
- **A gallery or watcher downloader page's list**: the page's
  "import options" button edits what new searches or watchers get (see
  the import options editor below), as do the "highlighted" boxes' own
  (and the highlighted search's file limit). Its
  downloader list is flat (the reference nests a site's downloaders and
  greys out ones that can't work). Its columns are as
  narrow as the reference's, which shows the status columns as single
  characters; their widths are fixed, but for the first, which takes
  the room left.
- **The "manage notes" dialog** now has the four live cog checks and
  both Notes Options controls. Current/all copying preserves Qt's JSON or
  naturally sorted human text, including an empty current note. Existing tabs
  retain selection; initial cursor preference affects newly created editors.
  The native multiline input exposes UTF-8 byte offsets while Qt uses UTF-16;
  regressions compare actual Unicode insertion outcomes and translated offsets.
  Viewer note middle-copy is implemented; its left-edit and right-hide gestures
  remain absent. Its notices ("Copied 2 encoded notes!") show beside the buttons rather
  than as a passing note over them, and the reference's shortcut to
  apply it isn't bound. Double-clicking beside the tabs doesn't add a
  note.
- **The "manage ratings" dialog**'s copy and paste are labelled buttons,
  its notices show beside them rather than as a passing note, and a
  paste's error is shown in the dialog's own panel; the error itself
  (after "the general error was:") is hydrus-rs's JSON parser's words
  rather than Python's, but for a bad service key's, which is Python's.
  A numerical rating pasted as a whole number (`1`) is copied back as a
  float (`1.0`), and an inc/dec count pasted as a fraction is ignored
  (the reference keeps it). Its controls are a fixed size, not the
  options' dialog rating size; its shortcut to apply isn't bound. An inc/dec
  control's middle click opens "edit value" with a spin box (0 to
  1,000,000) as the reference does.
- **The "manage times" dialog**'s date-time editor takes the date and
  time typed ("yyyy-MM-dd", "hh:mm:ss.zzz") rather than from a calendar
  and a time box, and the cascading step as milliseconds rather than
  hours, minutes, seconds and milliseconds. Its paste reads timestamps
  and the plain date forms ("2023-11-01", "2023-11-01 12:30:00",
  "2023-10-05T01:02:03.456") but not every date string the reference's
  date parser reads ("yesterday", "7/18/2023 8:32:00AM"). Its copy menu
  is a popup of the same entries; its notices show beside the buttons;
  its errors are in the dialog's own panel, and a JSON error's own words
  (after "the general error was:") are hydrus-rs's. The file modified
  time is changed on disk straight away, not in a cancellable job.
- **Forcing filetypes** renames the files straight away rather than in a
  cancellable job, and a file that can't be moved is copied but its old
  copy isn't queued for cleaning up as the reference queues it.
- **The "manage urls" dialog**'s copy and paste are labelled buttons
  rather than the reference's icons, and its questions are asked in the
  dialog's own panel. Its list sorts and selects as hydrus-rs's lists
  do (one column, "url").

- **Dragging across a list's rows with ctrl held** selects the range
  from the row pressed, as a plain drag does; the reference's adds the
  range to what was selected (or takes it away).


The search-undo entries (additions, removals, clear search history) act as the
reference's do but show no status-bar tooltips. After "Clear History" the
pages > history menu shows only that entry, without the reference's leading
separator.

## The media viewer (`hydrus-gui`)

- **The top hover frame's drag button shows the file in your file
  browser instead.** The reference's lets you drag the file out to other
  programs (a chat, a web page); Slint can't start a drag out of its
  window, so the button in that place shows the file, selected, in
  Explorer, Finder or your desktop's file manager (its FileManager1 D-Bus
  service, as the reference's show-in-file-manager package uses, or else
  the folder opened), from where it can be dragged. For the same reason
  the open menus offer "in file browser" whether or not hydrus's advanced
  mode is on (the reference offers it only in advanced mode). Dragging
  out for real is jkendall327/hydrus-mine#26.

## The duplicate filter (`hydrus-gui`, `hydrus-duplicates::statements`)

Its comparison statements are checked by
`crates/hydrus-duplicates/tests/comparison_statements.rs` against the
reference's (`oracle/dump_comparison_statements.py`).

- **A batch is fetched in one read and committed in one transaction.** The
  reference searches the potential pairs in throttled fragments, re-reads
  the whole search space after every commit, and commits four decisions per
  round trip; that is most of why it is slow.
- **Going back undoes exactly what the decision did.** The reference, going
  back, forgets that the first file was to be merged or deleted twice (a
  typo for the second file), so a later pair with the second file can still
  be skipped as dealt with.
- **A batch's first pair is skipped if it can't be shown**, as every later
  pair is. The reference shows it.
- **"software/source metadata" is listed.** The reference makes the
  statement but looks it up by another name, so never shows it.

## Duplicates auto-resolution (`hydrus-duplicates`)

Checked by `crates/hydrus-duplicates/tests/reference_run.rs` on
`oracle/fixtures/auto_resolution_run.json` (the reference's suggested rules,
some fully automatic, run over generated pixel-perfect and near-duplicate
families, with an approval and a denial in between), and the reference's
database after that run migrated (`legacy_db/auto_resolution.tar.gz`).
Stored rules are checked by `oracle/fixtures/auto_resolution.json`, and the
in-memory predicate tests their comparators use by
`oracle/fixtures/media_tests.json`.

- **Pairs are tested in order of their kings' hashes.** The reference tests a
  rule's pairs in whatever order its table gives. The order matters when
  actions interact (a merge changes which pairs are left), so both sides
  are pinned to the same order when recording.
- **A pair's orientation is random, as in the reference**, when both ways
  round pass; tests pin it (no shuffle) on both sides.
- **A rule's queue is reconciled with the potential pairs before it works.**
  The reference moves pairs between statuses whenever potential pairs change
  anywhere. The queue a rule works from is the same either way; the counts
  shown between runs can lag until the rule next works.
- **Visual duplicates are computed bit for bit as the reference does**
  (`crates/hydrus-media/tests/visual_data.rs` on
  `oracle/fixtures/visual_data.json`): OpenCV's 8-bit blur, area resize and
  RGB to Lab conversion (with its own cube root) are reproduced exactly,
  and the edge map's float blur and resize in the operation order of
  OpenCV's AVX2/FMA build, so every histogram, edge map and verdict matches.
  On a CPU without AVX2 and FMA the reference's OpenCV takes other code
  paths and its floats can differ in the last bit; ours are the same on any
  CPU (only slower there).
- **The rule editor's preview** takes its sample in one search, the
  pairs with the biggest smaller file first, where the reference fetches
  random fragments of the domain's pairs (sorting each); "pairs searched"
  counts the domain's pairs within the rule's distance. A double-clicked
  list with no local pair says so under the tabs, not in a dialog.
- **The "review actions" window** lists a rule's pending pairs by their
  groups; the reference lists them in its table's order (when they were
  queued), which hydrus-rs doesn't keep. A double-clicked pair with no
  local file says so on the terminal, not in a dialog. The rule preview's
  lists select as the reference's do, and their right-click menu shows the
  selected rows' files in a new page. Approving and denying work four pairs at a
  time off the UI thread, showing "approving: 4/12" on the button and, after
  four seconds, in a popup, as the reference does; the popup's text is
  checked between chunks rather than live.
- **Jpeg quality is read from the file's header** (its quantisation tables
  and sampling factors, as Pillow reads them), for "A has clearly better
  jpeg quality" and "is a progressive jpeg".
- **Time budget.** Without a GUI we are never idle: the active-time work and
  rest settings apply, and a rule with nothing to do is checked every
  minute rather than woken by new pairs.
- **URL class predicates match by the class's name** as it is now; the
  reference tests the copy of the class stored in the rule.

## Import folders and sidecars (`hydrus-download::folders`, `hydrus-parse::sidecar`)

Checked by `crates/hydrus-download/tests/import_folder.rs` on
`oracle/fixtures/import_folder_run.json` (a folder with `.txt` and `.json`
sidecars, filename tagging, a duplicate, an unsupported file, a file still
being written and a subfolder, whose new files are moved and duplicates
deleted), migrated from the reference's database before its run; by
`crates/hydrus-legacy/tests/sidecars.rs` on `oracle/fixtures/sidecars.json`
(400 random routers between sidecars, run on random sidecar files); and by
`crates/hydrus-legacy/tests/string_processing.rs` on
`oracle/fixtures/string_processing.json` (sorting and tag filtering steps).

- **Files are always imported from a copy**, as the reference does with its
  default "copy files to a temporary folder before importing" on; turning
  that off has no effect.
- **Filename tags aren't filtered by "storage" tag display filters.** The
  reference applies them, but its interface has no way to set one.
- **Import folders last saved before v7 of the format aren't converted**
  (they are reported). The reference saves a folder each time it checks it,
  so a folder in use is at the current version.
- **Progress is saved after every file**, rather than every ten minutes.
- **Nothing is published to a page.** The files a check imports are
  offered in a popup ("publish files to a popup button"), and its work and
  errors are shown in popups, as the reference's are; "publish files to a
  page" does nothing yet.
- **Changes from the command line are noticed within a minute** (the
  reference is told of changes by its dialogs).
- **Among tags whose human-sort keys are equal** (e.g. `straße` and
  `strasse`), a tag filter step's order is ours; the reference's is Python's
  set order, which differs from run to run.

Export folders (`hydrus-download::export`) are checked by
`crates/hydrus-download/tests/export_folder.rs` on
`oracle/fixtures/export_folder_run.json`: a regular export into subfolders
named by a phrase with `.txt` and nested `.json` sidecars, a synchronising
export of symlinks (clearing out what else was there), and an export that
deletes its files from the client, migrated from the reference's database.

- **Which filesystem a folder is on** decides whether filenames follow
  Windows rules. On Linux it is read from the mount table, as psutil
  does; on macOS it isn't known (so only Windows itself, or the "always"
  option, applies those rules), as when psutil can't tell.
- **Searches with "OR" as the search type** run as "AND" (every predicate
  must match); the reference's export folder dialog doesn't offer "OR".
- **What a run exported and removed is logged**, as well as shown in its
  popup while it works.

- **Where the filesystem ignores case, a moved sidecar keeps the
  lower-case spelling** (`a.png.txt`): both spellings seem to exist there,
  and we take them in the order sidecars are read. The reference, written
  for Linux, takes them in its set's order, so either may win.

## Simple downloaders (`hydrus-download::queue`)

- **A simple downloader page** uses an "edit formulae" button beside its
  chooser rather than a cog menu. The saved list supports editing, removal
  and adding defaults, plus reference PNG and clipboard-text import/export.
  Download controls sit under its boxes, not inside them.
- **Formula editors** support all six kinds, including recursive nested/zipper
  children and selectable inherited examples. Formula descriptions use compact
  typed summaries. The test panel accepts directly editable document text and
  key=value context lines, with automatic previews and background URL fetching.
  File fetch controls remain deferred. Native URL fetch controls use an inline URL
  field rather than the reference popup. Rule attributes use
  named fields and a list (activate a row to remove), rather than the
  reference's dictionary dialog.

Checked by `crates/hydrus-download/tests/simple_downloader.rs`,
`crates/hydrus-legacy/tests/simple_formulae.rs` (against
`oracle/fixtures/simple_downloader_formulae.json`),
`crates/hydrus-legacy/tests/gui_sessions.rs` (its pages in sessions) and
`crates/hydrus-gui/tests/gui/simple_downloader.rs`, and the formula editor
model/GUI tests against `oracle/fixtures/formula_editors.json`.

## Local imports (`hydrus-download::queue`)

Checked by `crates/hydrus-download/tests/local_import.rs`.

- **A local import (the reference's "import" page, `HDDImport`) is an
  import queue the daemon works**, as it does a URL downloader page's: its
  files are imported in order from their paths, each with its modified
  time as its source time, a missing one vetoed ("Source file does not
  exist!"), and, if the import says, each one in the database afterwards
  deleted (to the recycle bin, if the options say) with any sidecars its
  routers might have read. Its metadata routers (sidecars) run on each
  file once it is in the database, as an import folder's do (an error is
  shown as the reference's: 'Trying to run metadata routing on the file
  "..." threw an error!'); the tags to add to each file (from an "import"
  page carried over from hydrus) are added as a downloader's are.

Checked by `crates/hydrus-gui/tests/gui/local_import_dialog.rs` (against
`oracle/fixtures/local_import_dialog.json`) and
`crates/hydrus-gui/tests/gui/import_files.rs`; the routers and the
deleted sidecars by `crates/hydrus-download/tests/local_import.rs`.

- **The "review files to import" window parses its paths as the
  reference's does** (the same rows, order, filetypes, sizes, progress
  text and files to import, folders with and without their subfolders,
  sidecars and `Thumbs.db` set aside), but **it has no file or folder
  picker**: paths are typed or pasted into a box over its list, or dropped
  on it or the main window. Its "add tags/urls with the import >>" button
  opens the "filename tagging" dialog. Its "sidecars" tab edits the
  routers in the sidecar editors' window from a button (the reference
  lists them in the tab), and shows times in UTC (the reference, local
  time); a router's rows go through its processor in the order the
  sidecars give them (the reference's go through a set, so a slice or
  rows the human sort ties come out in no set order);
  the import-folder cached-path child now edits a complete private queue in
  memory and accepts it into the manager draft. Cancelling that child or the
  manager does not mutate the live cache; accepted edits survive cancelling
  only an existing folder's fields editor. Folder managers now acquire a
  crash-safe transient pause request before waiting for active workers to
  finish, then read the draft while holding an exclusive activity lease.
  Both requests and activity leases work across processes sharing the store.
  `oracle/record_folder_manager_lifecycle.py` records the real Qt manager's
  temporary pause, wait message and completion, and restoration of an already
  paused or unpaused state after Apply, Cancel and exceptions. Export management
  also notifies its scheduler in `finally`; import management notifies only on
  Apply. Native coordination uses owned file locks rather than Qt global
  running flags and temporary Boolean overwrites. It preserves live user pause
  changes, offers Cancel while waiting in the manager window, and releases
  requests on failed acquisition or save. Scheduler wakeups poll the inert
  request-file timestamp at one-second intervals instead of Qt publications.
  A crash releases the OS leases automatically; leftover unlocked files do
  not pause workers. Cancellation interrupts work between files, not inside
  an individual copy/import or database query, as in the recorded reference.
  Shared file-log menu parity (including all bulk-action questions and ignored
  retry regex filtering) remains independently incomplete.
  its simple tag lists use an owned shared autocomplete editor plus direct
  paste buttons, rather than Qt's inline autocomplete and tag list. The native
  child has Apply/Cancel within the filename draft, and holds the service/file
  selection fixed. Selected-file union edits preserve untouched per-file tags;
  explicit entry, autocomplete paste, direct paste and removal follow the
  recorded Qt distinctions. Unavailable clipboard text preserves the native
  draft and displays an error; Qt shows a critical message then raises an
  uncaught TypeError in this handler. The shared autocomplete's recent-tag
  history and complete tag-list keyboard/context operations remain separately
  incomplete. Advanced quick namespaces and regexes now
  use lists with the reference's accepted-rule actions and literal field values.
  The quick-namespace child and deletion question occupy the owning native
  window rather than separate Qt dialogs; namespace/regex header sorting is
  local to that editor, while Qt remembers its list-column state globally.
  Validation error details come from the native Python-compatible regex engine.
  Both inputs expose the shared component/help/favourites menus; native popup
  presentation omits Qt bold headings and action hover tooltips.
- **There is one review window at a time**: files dropped on the main
  window while it is open join its list, where the reference opens a
  second window.
- **Dropping files is not checked by eye**: the test drives the drop
  handler itself, since the X server the GUI is checked under has no
  drag source.

## Network requests (`hydrus-net`)

- **Pages are decoded with the web's own decoders.** A charset the server
  states is used as the reference uses it; otherwise, where the reference
  asks chardet, we take valid UTF-8 as UTF-8, then a `<meta>` charset, then
  chardetng's guess. The two guessers can disagree on short pages in legacy
  encodings.
- **A resumed download learns the file's size from `Content-Range`.** The
  reference takes the first response's `Content-Length`, which for a
  partial (206) response is only that piece's size, and so can stop early.
- **Requests may use HTTP/2** with servers that offer it, and ask for
  gzip, deflate, brotli and zstd compression. The reference's `requests`
  speaks HTTP/1.1 and asks for what it has installed.
- **Redirect targets are encoded by the URL parser** rather than by
  `requests`' `requote_uri`; both percent-encode what a URL can't contain.
- **Changed network options apply within a second.** The reference reads
  its options as it goes; the daemon reads the store's network and
  downloader options and bandwidth rules again each second, and uses them
  from then on (new requests use new timeouts, proxies and HTTPS checks,
  and the new job limits; a request already going keeps its slot).

## Downloading (`hydrus-download`)

- **An unexpected failure's note is its message**, without the Python
  traceback the reference appends.
- **Oversized downloads are refused when imported**, not while downloading:
  the reference stops a download as soon as it passes the file filtering
  options' size limits.
- **Bandwidth rules and usage are the reference's**, checked against its
  own classes (`oracle/dump_bandwidth.py`), and migrated. Two differences:
  a subscription that has files to get but not quite the bandwidth to start
  is looked at again a minute later at the soonest (the reference's
  "waiting estimate" can be zero then, and its loop would retry at once),
  and the usage of the last minute before a crash is lost (it is saved
  every minute and on stopping).
- **Watchers are grouped by page name only.** A watchable URL sent by
  `/add_urls/add_url` starts a watcher on the named watcher "page" (by
  default "watcher") as the reference's does, but there are no pages to
  show yet, and a check that errors pauses the watcher without the
  reference's five-second status display.
- **A pause switched from the command line takes up to half a minute** to
  reach a running `hydrus serve` (it looks again that often while paused);
  the reference's menu switches act at once.
- **Nothing is published to a page.** A subscription's popup while it
  works, its messages, and the new files it publishes to a popup button
  are shown as the reference shows them (and the messages logged). In its
  popup, the download it is doing has no stop button of its own (the
  popup's cancel stops the subscription where the reference's would), and
  goes when the download ends rather than ten seconds later.
- **Subscription changes made from the command line reach a running
  `hydrus serve` within five minutes.**
- **Full subscription exchange transport** supports modern reference container 90
  JSON and PNG without dropping query history or cached header metadata. Legacy
  subscription type3 versions1–10 now import through the actual list. The original transport menus are wired; clipboard text and JSON/PNG import menus now
  add to the staged list directly. The legacy explicit exchange callback still
  opens its former review child. Clipboard PNG
  image precedence and PNG list drops remain absent. Historical file-cache
  versions 1–7 now upgrade within the exchange codec, with recorded order,
  timestamp, note, count and example preservation. Later historical caches can
  contain duplicate identities; native queues cannot preserve those, so these
  imports fail explicitly instead of dropping entries. Version-1 float/complex
  notes requiring Python `str()` also fail explicitly; text, integer, boolean
  and None notes are supported. The codec applies generic URL encoding without
  a client's URL-class configuration; custom class-specific rewrites remain
  outside this recorded slice. The read-only database importer's cache decoder
  remains version 8 only.
  Missing histories now ask the original message, title and decisions before
  staging; accepted missing logs are initialised empty directly on Apply. The list owner now stages modern imports and
  persists both histories. JSON file export/overwrite and multi-file JSON/PNG
  import are wired. Known unrelated types now warn after permitted objects are
  added, including within nested lists. File selections keep their accepted
  prefix on unreadable/invalid files; future-version failures warn once and
  continue later files. Missing-history rejection continues the remaining
  package, and the last accepted row remains selected. Native exception bodies
  differ from Qt, and registered unrelated objects are classified without fully
  validating each type's internal payload. Each file/package has the existing
  16 MiB/4096-object limit. Unsupported subscription payloads still fail loading
  their entire package rather than partially decoding its histories. Staged and saved reset/retry exports now refresh the
  original file-count/example caches and forget hashes of retried files. Fresh
  native query exports initialise counts/examples; gallery and velocity caches
  without a retained reference header remain unsynchronised. This slice does not
  complete subscriptions-exchange.
- **The manage subscriptions dialog is a first pass.** It lists the
  subscriptions and can delete, pause/resume, scrub delays, check
  queries now and select by query text, add and edit subscriptions,
  merge, separate, lowercase, retry, reset, and overwrite downloader and
  checker options, and deduplicate. Import options can be copied as the
  reference JSON container, pasted and cleared within the dialog's draft.
  Subscription export/import opens a staged modern-container child (remaining
  exchange modes are noted above). Multi-group merge now stages only primary/name
  decisions until every group is answered, matching the actual Qt cancellation
  boundary (`oracle/fixtures/subscription_merge.json`). All absorbed owners go
  before casefolded name allocation; surviving original names remain reserved,
  including a primary's old spelling on a case-only rename. Like Qt, compatibility
  uses downloader names even when their stored keys differ, settings come from
  the primary, and overlapping query texts keep their separate histories. The
  existing native bulk transaction moves queue identities before deleting
  absorbed subscriptions; no histories are rebuilt or deduplicated by merge.
  It doesn't reckon bandwidth waits (the
  error/delay column is empty unless the subscription is delayed). It
  doesn't pause subscriptions while open, as the reference does: "apply"
  writes only what the dialog changed, so a subscription the daemon ran
  meanwhile keeps what the run found, unless the dialog changed the same
  query. Add and overwrite downloader use a separate gallery list; Slint
  has no native modal-parent API, so the subscriptions window disables its
  controls while that list is open. Like the editor's existing chooser,
  it currently flattens hidden and non-functional galleries into one list.
- **The edit subscription dialog** has no multi-site downloader warning, and
  no "additional tags" or file log compaction number in the query editor.
  Its downloader choice is one list (the reference puts the downloaders
  not on show, and those that don't work, under further entries); its
  retry buttons are two buttons where the reference has a menu. Editing a
  query's text to differ only in case is allowed (the reference refuses
  it, as a clash with itself), and renaming a subscription to differ only
  in case doesn't add " (1)" (the reference counts its old name as
  taken). Its queries' bandwidth waits ("recent delays") aren't reckoned.
- **The sidecar editors** edit a router's destination in a window of its
  own (the reference embeds it in the router editor), list "Which
  type?"'s descriptions in its message. JSON object names now use the actual
  ordered add/edit/delete/reorder workflow with staged owned text children;
  the native list/text-child geometry differs from Qt. JSON sidecar formulae
  use the reusable HTML/JSON editor;
  router testing uses per-source tables instead of a notebook. Router queues
  import/export clipboard text and PNGs through the shared staged review window,
  rather than Qt's separate chooser dialogs. Router import now preserves the
  compatible subset of recognised mixed objects and shows the reference's type
  and direction warnings; ordered multiple-PNG imports stop at the first failed
  file and retain earlier successful packages for review. The subset is appended
  only after accepting that review, so cancelling review discards it; Qt appends
  each successful package immediately to its still-unsaved queue. Count notices
  are represented by the staged review rather than separate per-file Qt notices.
  Source/destination editor samples, filename conversions, JSON formula data and
  timestamp stubs round-trip. Non-stub timestamps and unsupported processors are
  rejected before staging rather than silently dropping information.
  Typed router/subsidiary exports now use the reusable title/description/width PNG
  child and retain the reference type/count/size summary in its header. Selected
  queues export as one bundle, as the actual non-named router control does. The
  separate export-each-object-to-PNGs dialog remains absent for named subsidiary
  parsers; Qt does not offer it for metadata routers.
  Export-folder search results
  are not yet supplied as media examples (manual exports are).
- **The string processor editor** receives starting strings from the sidecar
  owner's first example, including source processors before their own processing.
  Import/export/paste use a shared
  text/PNG review window. Unsupported mixed packages are rejected atomically;
  the reference may append permitted entries and warn about rejected ones.
  "add" lists its kinds' descriptions in
  its question. A sorter's or match's error for a regex that won't
  compile is in hydrus-rs's words, not Python's ("That regex did not
  work! ..."). Match and sorter regex favourites use a description chooser
  that copies the phrase to the clipboard, plus a shared manager. The .* control
  presents component menus in a compact popup palette rather than nested
  submenus. Help links remain absent. Favourite phrase and description are edited
  together in a row form, where the reference uses sequential dialogs.
- **The string converter editor** persists the last accepted conversion
  in its owning store and reads preserved reference options until a native edit.
  Store-less embedded API callers retain the previous in-memory fallback. Conversion regex fields have component/replacement group
  controls; their help and favourites menus remain absent. Its date phrase link
  is shown as text. Date conversions execute and update live previews. Advanced parsing
  uses Jiff's diagnostic reasons rather than Python's; English directives and
  common ISO/English automatic dates are supported. The easy parser supports
  relative English units (seconds through years), now/today/yesterday/tomorrow,
  but not dateparser's full multilingual and fuzzy grammar. Locale-dependent
  date phrases use English/C forms. Advanced parsing validates Python's
  six-digit microsecond limit, ignores recognised UTC/GMT/system timezone names
  as Python does, and rejects year zero and non-Python directives. Jiff's
  compatible local-time resolution chooses the earlier repeated time and shifts
  nonexistent times forward; platform-specific Python choices can differ at DST
  transitions. Platform-specific strftime extensions such as %s are outside the
  supported format grammar. A bad hex or base64 string's error is in
  hydrus-rs's words too where Python's says more.
- **The import and export folders dialogs**: an import folder's filename
  tagging is added for a tag service chosen from a list beside "add" (the
  reference asks which in a dialog), and edited in the "filename tagging"
  dialog's boxes (see "review files to import"). An export folder's
  query is typed as the Client API's tags rather than through the search
  autocomplete. Its example refresh now queries actual stored media and supplies
  up to 25 results to the sidecar test panel. Unlimited example queries use stable
  file-id order (the reference returns a Python set); limited local queries use
  the recorded size ordering. All-known-file limited queries use the native
  search's deterministic size ordering rather than the reference's random sample
  when that domain cannot sort at database level. They
  don't pause the folders while open, as the reference does.
- **Options > maintenance and processing**'s repository processing,
  sibling/parent sync and deferred table delete timings are kept and
  edited, but nothing reads them yet: hydrus-rs has no repository
  processing, and it syncs siblings/parents and drops tables as it writes
  rather than in background work. Their boxes lack the reference's one-line
  explanations, aren't collapsible, and aren't imported from a legacy
  client's options.
- **Tags > sync**: hydrus-rs applies siblings and parents as it writes, so
  "sync now" always finds nothing to do, and its idle/normal switches are
  kept without a background-work consumer. The review reports each service's
  current sync state but has no pending-work scheduler to control.
- **Database > db maintenance**'s deferred delete switches are kept without
  a consumer: hydrus-rs drops tables as it writes.
- **The duplicates page's preparation tab**'s "regenerate search tree"
  and "regenerate search numbers" ask the reference's questions but then
  only refresh: hydrus-rs builds its search index afresh and counts
  searched files directly, so there is no cache to regenerate. The resync
  reports in one finished popup, not a cancellable progress one. Working hard tells `hydrus serve` to search whatever
  its idle and normal time switches say (it has no idle time of its own).
  The tab's name always hides the percentage once over 99% done (the
  reference's option for it isn't kept). The auto-resolution tab can't
  review rules' actions yet, and a rule's status can't say
  which rule `hydrus serve` is working on ("searching", "resolving"): it
  says "working".
- **The auto-resolution rule editor** takes its searches as terms typed
  a line each (as the Client API reads them), where the reference has
  search autocompletes and a live count of pairs; its location button
  opens the "multiple/deleted locations" list straight away (the
  reference's offers a menu of single domains first). It has
  no import/export/duplicate of rules or comparators, and its custom
  merge options are edited in their own window (the reference embeds
  the editor). A relative comparator's time delta and range are
  in milliseconds, where the reference has a time widget.
- **The file log window** can't yet import new sources, export them to
  a png, search for the selected URLs, or do its advanced entries (these
  are greyed out); its "additional urls" don't show the URL a URL class
  would actually fetch or refer from; trying a previously deleted file
  again doesn't offer to clear its deletion record.
- **The import options editor** keeps typed-line fields for the tag filtering
  whitelist and additional tags, with a detached shared write-tag autocomplete
  editor for both lists. The reference embeds its tag inputs. Note names remain
  typed lines, and note renames use "parser name -> saved name" rather than the
  reference's two-column list.
  The tags page's "set a filter for already-exist test" isn't there.
  Locations take one destination (the reference's takes several), and
  presentation's location is all my files or all local files. It has no
  copy, paste or favourites buttons, and always lists kinds as the
  reference's "simple mode" does (hydrus-rs has no option for it yet).
- **The merge options editor** asks its select dialogs as a row of
  buttons (no service or action preselected), and edits the note merge
  settings in a box of its own rather than a dialog.
- **The tag filter editor** imports/exports reference JSON with an additional
  inline clipboard/file panel before naming an import. It does not yet offer
  repository serverside tag filters in the load menu (remote repositories are
  not functional), or explanatory control tooltips. Favourite names/save/delete persist
  immediately as in the reference; filter changes reach the owner on Apply.
- **"clear and load" a session**: when pages object to closing, the
  question has "yes" and "no" (the reference's also has "no, but show me
  the pages", and its "yes" is only enabled after a moment).
- **The search log window** can't yet export URLs to a png, import new
  URLs, or export the selected page objects (greyed out).
- **A subscription query's logs**, opened from the query editor, change
  the query at once; the reference edits a copy that the dialogs'
  "apply" keeps or "cancel" drops. A query added in the dialog has no
  logs to open until it is applied. Likewise an import folder's file log.
- **Subscription concurrency** now uses the persisted 1–100 maximum,
  with the reference's default of one, single-flight subscription IDs,
  live changes, global pause admission checks and a 120-second finished-run
  buffer. Native Options and subscription drafts do not pause the daemon
  merely because an editor is open; the reference subscription manager has
  a separate pause-for-editing state. Concurrent script/import changes still
  use the existing optimistic merge boundaries rather than a global editor lock.
- **Import options that run a program on each imported file are kept but
  not run yet.** The migration warns where they are set (which defaults,
  subscriptions, import folders or downloader pages).
- **There is no browser impersonation** (the reference's optional
  `curl_cffi` connections); requests are always plain ones.

## Downloader definition editors

- URL classes and single/nested gallery URL generators have native lists and
  rule editors. Their duplicate button creates new keys and unique names.
  Domain lists and regex lists use one rule per line, and nested generators
  select members with checkboxes. Page/content parsers and direct URL-class links
  have native editors and reference JSON/PNG import/export. Login editors
  remain follow-up work.
- Timestamp content editing now uses the original single source-time choice,
  including normalisation of unset/obsolete saved types and real date-converted
  metadata reaching file seeds. Native shared parser test panels still present
  compact raw content summaries instead of Qt's per-kind human-readable result
  decorations; this timestamp slice does not complete those parent test panels.
- Invalid example details use the native URL rules' error wording. The
  reference retains stale referral/next-page examples after a match failure;
  the native editor clears all derived output. A changed list asks before
  cancelling; cancelling an individual child editor discards its draft.
  Association notices appear inline, and dependent-generator deletion
  confirmations are combined into one question before deleting the selection.
- Gallery paging rejects invalid index/delta input on Apply. Delta is 1 to
  65536, as the reference's spin box allows. The native editor exposes zero
  based path indices or a query parameter name directly.

## URL classes

- **A URL missing a required query parameter is reported by that
  parameter's name.** The reference's message names whichever parameter
  its loop looked at last (which varies from run to run).

## Downloader parsing (`hydrus-parse`)

Checked by `crates/hydrus-legacy/tests/formulas.rs` on
`oracle/fixtures/formulas.json` (random formulas on random documents) and
`crates/hydrus-legacy/tests/page_parsers.rs` on
`oracle/fixtures/page_parsers.json` (random page parsers, with subsidiary
parsers and every kind of content parser, on random documents).

- **`<template>` contents are parsed by the current standard.** HTML is
  parsed by the current HTML standard's algorithm (html5ever), as browsers
  do. The reference's html5lib predates parts of the `<template>` rules: it
  drops table cells and rows inside a template and moves a template out of a
  table, so formulas walking such markup find different tags there. Real
  pages rarely put table parts in templates or templates in tables.
- **A JSON "deminify" rule whose references form a cycle is a parse error.**
  The reference recurses until Python gives up and the parse crashes.
- **A parsed time that doesn't fit in 64 bits is ignored.** Python's
  integers are unbounded, so the reference keeps a "time" like 10^21 seconds
  and fails later, when the database can't store it. We treat it like any
  other unreadable time.

## Serving files (`/get_files/file`, `/get_files/thumbnail`, `/get_files/render`)

Byte ranges are checked by the `file_ranges` conformance scenario, renders by
`crates/hydrus-api/tests/render.rs` on `oracle/fixtures/render.json`.

- **Byte ranges the reference fails on are answered.** A range ending at
  exactly the file's size is clamped to the file (the reference promises a
  byte more than it sends, and the response never completes); a range
  starting past the end with no end gets the whole file (the reference's
  response never completes); a suffix longer than the file gets the whole
  file (the reference fails with a 500).
- **`Content-Range` of a suffix range (`bytes=-100`) names the range's real
  end**; the reference writes the suffix length there.
- **Rendered WebP is lossless.** The reference encodes lossily at the quality
  asked for (80 by default) unless it is over 100; only a lossless encoder is
  available to us, so every WebP render is what the reference gives for a
  quality over 100. PNG and JPEG renders decode to exactly the reference's
  pixels. The same goes for ugoiras rendered as animated WebP; as APNG
  (their default), their frames and timings are exactly the reference's
  (`oracle/fixtures/ugoira_render.json`).
- **A ugoira frame without a duration takes the default (125ms)** when its
  timings (in a note) are fewer than its frames; the reference fails with
  a 500.

## Popups (`/manage_popups/*`)

Checked by the `popups` conformance scenario.

Popup width Options controls are staged, imported and persisted, and reach the
real oldest-ten popup consumer. Both clients capture a card's width policy at
construction; Apply affects newly displayed cards while existing cards retain
their limit. Qt uses padded averageCharWidth with a sample-text fallback; native
Slint measures the bold-font sample because it has no averageCharWidth API.
Approximate pixel widths, wrapped-text size hints and the summary-bar minimum
therefore differ with the platform font/layout. Both support variable width up
to the cap, forced fixed width, and the gauge's 90% minimum. This two-control
slice does not add freeze-on-other-monitor, freeze-when-minimised or Client API
cookie/header notification switches. Popup/notifications parents remain Partial.
Actual Qt staging, bounds, loaded raw values, serialization/reopen and existing/
successor card geometry and real pending-eleventh queue admission are recorded
by `oracle/record_popup_width.py`. Authored
native/store/model regression source awaits hosted CI; no Rust runs locally.

- **The popups are the store's**, so the daemon's Client API and the
  client share them: the client shows the oldest ten ("in view", as
  `only_in_view` lists them), whether or not it is open, and a dismissed
  popup leaves the list at once (the reference's GUI clears dismissed
  popups on its next refresh, within a second or so).
- **The daemon's own popups are, so far, subscriptions', import folders'
  and export folders'**: their work as it goes, messages, errors and new
  files. The reference also shows other jobs at work as popups (downloads
  from the menu, maintenance, database jobs); hydrus-rs doesn't yet.
- **Popups outlive the daemon, unless their work does**: those for work
  going on are forgotten when the daemon stops or starts (as the work has
  stopped), but messages and finished work stay until dismissed. The
  reference's popups all go when it closes.
- **Live popup actions stay in their producer process.** Clipboard payloads
  persist with the job, while callables and questions use owner-qualified SQLite
  requests. The native clipboard, callable and yes/no controls consume these
  current values. A yes/no click immediately finishes/dismisses the row and
  commits its reply; the retained producer reads it afterward. Callables execute
  serially outside producer locks on its weak polling task (up to 250ms later),
  with current callbacks and ownership checked before each effect. Question and
  action changes publish synchronously. Rebinding or accepted main-window close
  retires unconsumed GUI calls, while committed answers survive. A producer drop
  or daemon restart removes executable handlers; finished clipboard messages
  remain copyable. This does not add reference-only maintenance/menu producers,
  modal job windows, popup freeze rules or download/cancel parent completeness.
- **A popup's `network_job`** says its URL, status, speed, bytes read and to
  read, whether it is done and whether it failed, as the reference's does;
  the popup/API shape does not yet carry the network review's typed wait
  reasons. Its connection/domain/server-bandwidth/engine flags still read
  `false`, `true`, `false` and `false`, and `total_data_used` is what this
  request has read.
- **An error's "traceback" is its text**, and its title is "Exception", as
  the reference titles the errors it raises itself: hydrus-rs has no
  Python traceback to show.

## Repositories (`/manage_services/*`)

- **Pending content can't be committed.** hydrus-rs doesn't talk to
  repository servers (the PTR, file repositories), so `commit_pending`
  checks the request as the reference does and then refuses it (422).
  Pending counts and forgetting pending content work as in the reference
  (the `manage_services` scenario, on the `repositories` fixture).

## Locking the database (`/manage_database/lock_on`)

- **The database stays open while locked.** The reference closes its
  database connections; we stop every read and write, move the write-ahead
  log into the database file and leave the connections idle. A copy taken
  while locked is complete either way, but on Windows the files can't be
  moved or deleted until the lock is released.

## Client API input checking (`hydrus-api`)

- **Booleans are not numbers.** `/edit_ratings/set_rating` rejects `true` or
  `false` for a numerical or inc/dec rating service (Python counts a bool as
  an int, so the reference stores `true` as one star).

- **Service review** currently uses a service dropdown in place of the reference's nested local/remote/type tabs. It shows native counts, id/key controls and refresh. The long service descriptions and repository/IPFS account administration remain unavailable. Local trash clear/undelete, double-confirmed deleted-file-record clearing and all three local rating-clear populations are implemented; bulk rating choices and confirmations use native inline controls rather than Qt popup menus/dialogs. Opening a replacement review retires the previous owner's pending maintenance confirmation.

- **Local service management** uses an add-kind dropdown and inline confirmation text rather than Qt popup menus/modal questions. Rating colours use validated #RRGGBB text fields with four live, independently interactive rating examples; named SVG configurations are preserved/edited, with rendering subject to the existing SVG support limits. Remote repository/IPFS/account edits remain unavailable here. Client API listener settings are available; HTTPS, normie Eris and external URL overrides are preserved imported values, with an explicit control to disable unsupported HTTPS. A concurrent registry change rejects Apply and asks the user to reopen the editor; expensive full count rebuilds run inside the atomic service transaction. Successful Apply refreshes displayed selection/viewer tags after source-service deletion, including a locked page whose files stay fixed.

## Manual file exports

Manual export uses the scheduled export folders' filename and sidecar code.
The preview paths, selected-name collision suffixes, removal question and
trash/export-and-close confirmations match `oracle/fixtures/export_files.json`,
as do filenames and copied bytes from the real reference export worker.
Windows also accepts a forward slash at the final folder/filename split, using
the reference's Windows filename sanitization for the preceding directories.
After a failed export, confirmed trashing now moves the fully exported prefix
to trash, matching the reference. Cancellation before deletion suppresses all
trashing, even after the last copy; deletion commits in batches of 64. Sidecars
run before copying. Native cleanup errors retain the original export error and
the count of earlier committed trash batches, rather than leaving an unhandled
worker exception. Exact-source Linux validation and independent rendered review
passed at `da6fb4d35` (2,201 tests, including 715 GUI and 67 media tests).
Evidence (checkpoint `da6fb4d35`, in git history before 2026-10-08). Qt pauses its worker on
critical-error acknowledgement before cleanup and can observe cancellation during
that pause. Native displays errors inline and can immediately begin already-confirmed
cleanup; the recorder intercepts the dialog and does not prove timing parity.
Native source/path preflight precedes sidecars and is stricter than Qt. Paths
are checked by components and canonical subfolders, and existing symlinks,
exports into managed file storage and overwriting a source pathname are
rejected. Copies replace destinations atomically through a sibling temporary file;
existing hardlinks are detached so other directory entries retain their bytes.
Existing sidecars are detached before routing for the same reason.

The window offers the reference's interactive pattern-shortcut clipboard menu;
the selected-files tags sidebar supplies actual display counts, local sorting,
copy/search/relationship/favourite menus and owner cancellation. Its generic
maintenance → regenerate tag display action, advanced experimental display-type
switch and full keyboard navigation remain unported. Native shortcut menus do
not reproduce Qt's bold heading or action hover tooltips.
Successful "Export and close" closes the review window. Native failure or
cancellation retains it for review; Qt closes its review frame after worker
completion (not the whole client). Cancellation completes
an in-flight copy before stopping between files. Progress is in the review
window rather than a separate popup job. Removing rows refreshes filenames
immediately; the reference retains cached paths until the phrase or directory
changes.

The imported export phrase, filename limits and default export directory are
reused. The exporting option supplies the starting manual directory; changing
one manual window does not change that option. Native settings keep resolved
paths rather than Qt's portable text, with imported relative paths resolved
against the source database and native relative input against its database.
The blank preference uses the home `hydrus_export` folder, including Windows
USERPROFILE when HOME is absent. Legacy manual trash preference and default
sidecar routes are not yet mapped; choices for these made in the window persist.

## Tag relationship editors

The siblings/parents editors share write-autocomplete, counts, decorated
suggestions, manual fetch and add-only paste with Manage Tags. The reference's
tag context menus, favourite/children tabs and default service-tab preference
are not connected yet. Import/export
are direct clipboard and .txt buttons rather than two popup menus. Relationship
rows are loaded synchronously when the dialog opens, so opening a service with
very many pairs can pause the UI; reference background fetch/progress states
remain to be ported. The port commits and recalculates display immediately,
so it shows that behavior instead of the reference's background-sync status.
Repository reasons and rescinds are supported, but account/moderator permission
warnings, moderator reason bypass and recent/fixed reason suggestions are not
implemented. Pending changes are persisted; uploading still depends on the
repository uploader's existing capabilities. Self-pairs imported from text are
reported and rejected rather than stored after the reference's critical loop
warning (the display graph ignores such pairs anyway). Batches creating loops
or conflicting sibling ideals are rejected with an explicit message; enter
the pairs separately to perform the ordinary automatic repairs. Already corrupt
reference graph cycles are traversed safely, but do not raise its detailed
pre-existing-loop warning. Manual background sibling/parent synchronization
remains separate future work.

## Detailed embedded file metadata (`hydrus-gui`, `hydrus-media`)

The sections and raw EXIF clipboard values are checked against
`oracle/dump_embedded_metadata.py`; the real panel's PNG note, visibility,
read-only text, sorting and basics tree are recorded by
`oracle/record_embedded_metadata_window.py`, including PDF document fields and
non-local behavior.

- HEIF, AVIF and JPEG XL embedded metadata is currently empty because the
  existing image decoder does not open those formats as Pillow's plugins do.
- Non-UTF-8 IPTC bytes are decoded lossily, rather than by the reference's
  `NonFailingUnicodeDecode` charset guesses.
- Malformed XMP that the XML parser rejects is empty where BeautifulSoup may
  recover a partial tree.
- A missing or unreadable local file shows its read error within the window;
  the reference logs an exception while opening the window with basics alone.


Tag display configuration uses immediate atomic graph/count publication instead
of the reference's background sibling/parent sync. The native application table
uses a zero source id solely to represent an explicitly empty queue; absent queues
retain the default of applying the service's own rules. Deleting the last source
from an explicit queue preserves its empty intent. Native settings preserve
unknown JSON fields and unedited service settings. The application
window uses ordered native lists and an inline source selector. Display/search
uses a numeric zero for the reference's nullable "always autocomplete" threshold.
Manual/background sibling/parent sync remains unimplemented.
Autocomplete configuration refresh preserves any open manage-tags draft; its
location editor always exposes the permitted file domains.
Display/search edits merge unedited services and setting areas from the current
database. Concurrent edits to the same area use the last successful Apply.


The native parser editor model supports all nine content kinds and typed test
context. Native page/content/parser-list and direct URL-class-link windows are available.
Recursive subsidiary creation/editing, deletion, separation and source-time sorting
are available. Subsidiary queues import/export complete standalone wrappers via
clipboard text and reference PNG, preserving separators, sorting, recursive
pages and inert editor data. Selected queues duplicate without replacing parser
keys; deleting selected rows asks the recorded confirmation. Invalid/mixed-type
packages are rejected atomically instead of partially importing valid rows.
Test-data
URL fetches use a window-local downloader engine: progress appears in the test panel
rather than the daemon job review. Its requests use the same headers, cookies,
network settings and accounting machinery, and their usage is merged safely with
the daemon. All six native formula kinds and subsidiary parsers
can be imported/exported; their editor-only reference data is preserved.

The URL-class links panel uses a parser chooser and explicit staged link/clear
actions. Downloader package import automatically links parser example URLs;
the reference API/redirect review tab remains deferred. API/redirect source
classes are excluded because their targets own
the parser. The temporary-variable content kind is also editable here, while
the reference page editor normally limits its creation to lookup scripts.

Changed parser links are rejected if another editor removed their URL class or
changed it to a kind that cannot own a parser. Reopening the links panel then
shows the current eligible classes; unrelated class edits are preserved.

Client API key edits made through native persistence now take effect on the
next authenticated request through a durable permission revision, including
existing sessions. Changed/revoked keys lose their previous restricted search
results; a stale in-flight search cannot restore them. The database unlock
endpoint authenticates its cached admin key while the native store is paused.
The detached GUI model saves a whole key edit only on Apply and rejects stale
concurrent editor state; Qt service review applies list actions immediately.
The access-key window opens separately from service review; the reference
embeds its list within that panel. API-request registration still uses
`hydrus api-keys listen` rather than the Qt capture-request dialog. Key-change
questions use an inline edit panel and generated-key button. Listener changes
may take up to one second; current requests drain for at most ten seconds
before restart. HTTP logs omit query strings and credentials. HTTPS is refused
rather than served as plain HTTP; normie Eris/external URL override fields are
shown as unsupported preserved values in plain text; unset external URL fields
read "not set".
Listener reconfiguration retains the same API state, so session keys continue
to use the current permissions after rebind; revocation still invalidates them.

The base-URL button prefers the daemon's actual listener over the service's
configured port to support native CLI overrides. It falls back to the saved
configuration when the daemon does not report a listening address.

Clipboard URL monitoring runs while the desktop is open. Fatal clipboard access
errors use the shared popup queue so they remain visible on downloader and
notebook pages. The reference watcher policy and toggle resets are replayed from
`oracle/fixtures/clipboard_urls.json`.

Login script types, bounded JSON/PNG interchange and credential/temporary-variable
validation are available through native script, step and credential editors.
Script/domain edits wait for parent Apply; advisory questions appear inline.
Request arguments, cookie matchers and VARIABLE/VETO response parsers are editable
and used by actual HTTP script tests and confirmed domain attempts. Domain add/script-choice and session-status/reset controls remain absent. Domain credentials reproduce validity/delay/activation behavior;
the domain list omits logged-in cookie expiry and displays future delays as raw
timestamps. Imported credential maps do not retain arbitrary dictionary iteration
order; entry checks follow the recorded normal-before-hidden control order.
Full preserved credentials are loaded without discarding fields; independent
script/domain Apply preserves concurrent edits to the other half of preferences.

## Network session and HTTP-header management

Cookie and HTTP-header editing uses detached native drafts with Apply/Cancel;
Qt cookie-list actions take effect immediately, while its header list is staged.
Changing a cookie's name/domain/path replaces the old identity, while Qt adds the
new identity and leaves the old cookie. Native editing preserves secure and
other attributes and exposes the secure flag; the reference's cookie editor
recreates a cookie without exposing these attributes. Native validation also
rejects invalid HTTP field names, cookie delimiters, relative paths and impossible
UTC expiry values; empty cookie/header values are accepted as valid HTTP data.
Expired cookies remain visible until manually removed, while Qt periodically
clears expired cookies when opening a session. Header duplicates use valid `-2`, `-3` suffixes instead of
Qt's human-name suffixes containing spaces. Automatic header approval uses a
separate desktop question window rather than a JobStatus popup. It watches fresh
daemon snapshots and displays one question per pending header blocking live jobs.
A Later/close action leaves approval pending and suppresses the exact header/job
set until either changes. A changed header must be reviewed anew; Qt's validation
process does not guard its answer against concurrent value edits. Requests resume
on the engine's next approval check (currently up to five seconds).

The browser can inspect imported service sessions and create domain sessions;
creating new service sessions and drag/drop cookie imports remain deferred.
Clipboard export uses the same five-field format as Qt, so secure and other
attributes are omitted and clipboard imports reset them; Netscape imports preserve
secure/HttpOnly instead. Cookie-list imports use the existing Apply/Cancel draft.
Native imports validate every cookie before changing any draft or store and reject
an entire malformed multi-file selection; Qt can leave earlier accepted files or
cookies in place when a later item fails. Imports are bounded to 8 MiB per input
and 10,000 cookie identities per file/clipboard; UTF-8 is required for files.
Netscape expired cookies remain visible but are never sent. Confirmation text uses
plain domain lines for large lists rather than Qt's compact summary layout. Empty sessions are
explicitly persisted in the native store. Refresh preserves a cookie/header
draft; reopen reloads committed changes. Browser create and confirmed clear
actions take effect immediately. Passwords and cookie values remain ordinary
local database fields, as in the reference.

## Bandwidth and current network jobs

Native bandwidth review uses numeric bytes/requests and seconds with a monthly
switch in one detached rules window, rather than the reference's nested amount
and time widgets. Rule usage and monthly charts are inline in the all-context
window, rather than Qt's separate per-context window. Monthly bars display each
month's exact byte label with proportional height instead of QtCharts axes.
Usage-age filters use request counts like Qt, exclude ephemeral usage contexts,
and offer custom seconds plus show-all and optional specific rules. The native
last-age preference is stored separately from imported Python options. History
deletion is transactional; per-context reset generations prevent old engine
flushes/snapshots from restoring deleted usage. An active daemon resets those
trackers and wakes bandwidth waiters at its next heartbeat. New explicit contexts
can be added by domain; existing subscription and service contexts remain editable
through their usage rows.

Current network-job review refreshes live by default (Qt starts with manual
snapshots). Native typed wait reasons distinguish pauses, header approval, wake,
bandwidth, domain, gallery, connection and server waits from transfers; Qt's
engine-position strings/debug menu use different categories. Selected-job debug
context/obeys-bandwidth details are inline, and cancel/override controls are
explicit buttons. Login-script waits remain outside the implemented engine.
Local snapshots are published by the daemon without requiring its Client API;
expiry disables stale controls and saved bandwidth history remains available.

## Tag migration

Migration opens from Tags > migrate, service review or Manage Tags. The global
entry uses the configured or remembered default tag-dialog service, without a
selected-file restriction; an unavailable service uses the migration editor's
existing fallback. The Tag Editing options page exposes both default-service
preferences.

Service-to-service tag migration uses a stable WAL reader snapshot and bounded
atomic destination batches rather than the reference's temporary source tables.
This prevents source-equals-destination deletions from skipping rows and fixes the
source membership for the entire job. A long job retains a SQLite read snapshot,
so concurrent writes can grow the WAL until migration completes or is cancelled.
One migration runs per open Store; a second job is rejected promptly so a normal
reader remains available to the UI. The reservation is released on error or cancel.
Display graph/count publication happens after each relationship batch, following
the existing native immediate-sync design; large graph rebuilds occupy the writer
but run outside the UI thread. Pause/resume waits after the current atomic batch,
as the reference does; cancelling also wakes a paused job. Closing the settings
window leaves its independently retained progress job alive. Native progress is
a separate window rather than an embedded message-manager popup; it follows the
reference phase/speed text, paused override, immediate cancel/dismiss controls
and delayed completion dismissal. The native timer checks the strict integer
three-second deadline every 80 ms; the reference manager/checker polls it.
Hydrus Tag Archive/tag-pair archive sources are read-only snapshots, including
hash-type inference, rather than opening the reference's writable archive job.
Native archive output uses the actual Python SQLite schemas and commits atomic
bounded batches; the reference commits its long archive transaction during job
cleanup. Cancellation retains the committed prefix. Existing hash/pair metadata
is revalidated and preserved. The reference's existing MD5 archive picker can
leave its SHA256 dropdown selected while disabled; native pins the actual archive
kind, avoiding relabeling its existing hashes. Source and destination must be
different archive paths. Native output does not run the reference's optional
post-job VACUUM/ANALYZE optimization.

SHA256/MD5/SHA1/SHA512 conversion uses known stored digests; unknown conversions
are skipped, while same-kind all-known archive copies retain unknown hashes.
Selected-file/domain filtering converts through SHA256. Pair gates use actual
current and pending storage mappings and the sibling terminal ideal, rather than
implied display counts. Count gates reload a fresh reader snapshot after each
source batch, matching the reference's current service counts and ideal chains
while source pagination remains stable. Repository migration retains
pending/petitioned content locally; uploading is outside this window's scope.
The reference's fixed Mass Migration Job reason is offered as an editable petition
reason in the native window. The speed label counts accepted source entries,
including destination entries already in the requested state, as the reference does.

Go freezes the entire migration request. Applying an already-open filter or location
child after the confirmation appears changes only the next job's settings.

## Downloader definition interchange

Downloader interchange uses text clipboard contents and selected PNG/text files;
clipboard bitmap and drag/drop ingestion are not exposed. Exported PNGs carry
the real reference pixel/payload format with a small plain header. A malformed
or unsupported item rejects the whole package, with an actionable error, rather
than the reference importer's partial skips. Payloads/pixels are capped at
16 MiB, bundles at 4096 objects, and recursive formats at bounded depth. Old
container versions and recent URL/parser/formula versions are upgraded;
earlier unsupported versions must first be re-exported by the reference client.
Unknown processing steps/conversions are rejected before staging because their
native execution forms cannot retain all original data. Native/runtime fields
take precedence over preserved auxiliary editor fields when exporting edits.
Mixed downloader package import accepts URL classes, GUGs, page parsers and login scripts;
standalone formulas/content nodes belong in their matching native editors.
Domain metadata packages remain unsupported here. The native mixed exporter
uses component checkboxes with dependency expansion instead of Qt's separate
Add choosers. Imports review a whole supported package; Qt additionally offers
optional per-object selection and skips unsupported objects. Mixed login script
duplicates ignore key/name; new imports keep script names, regenerate keys,
retarget existing matching example-domain links and preserve current domain
credentials/activation/delays without configuring new example domains. Standalone
login-list imports retain their separate nonduplicate-name policy. The actual
Qt mixed-package recording also captures a repeated nested GUG import creating
an additional nested generator after child keys change; native retains its
existing remapped dependency duplicate checks. This login slice does not claim
domain metadata, bitmap/drag ingestion, or the wider downloader exchange parent.

Tab context menus expose close, select, move-page, sort-pages and send-down submenus,
rename, duplicate, collapse, grouped close and per-notebook saved-session
append/save actions. The remaining page-selector child workflows are deferred. As in
the reference, a page not opened/initialised contributes zero to the size sort;
kept file counts and persisted importer progress still participate in count sorts.

Tab refresh and advanced page-weight information now follow the reference popup.
Recursive refresh skips unopened descendants and preserves notebook selection;
importer refresh re-sorts media without starting paused transfers. Duplicate
sidebar counts are read when the active page is rendered rather than dispatching
an independent background sidebar job.

The tab popup's new-page actions now target their notebook and preserve an
explicit insertion anchor through the chooser. The four default insertion
positions import and apply through GUI Pages. A removed destination or insertion
anchor is rejected before creating importer queues. Unlike the reference's
retained `_next_new_page_index` after a cancelled chooser, native cancellation
clears that pending position so it cannot affect a subsequent page creation.

Named GUI session saves now retain selectable immutable snapshots; automatic
`last session` synchronization writes the live session; a separate historical
autosave timer now observes the configured period and idle-only preference and
suppresses unchanged saves. Idle input tracking covers every native application
window and the shared Client API activity marker; system-wide mouse movement
outside these windows remains unobserved. Historical
backups from imported legacy databases are not migrated; the current imported
session is retained as the first backup when overwritten. Backup loads start fresh transfer/live-job state while retaining saved queue
settings and file/gallery logs. Early native snapshots without importer-state
data cannot restore queue-backed pages; snapshots recorded by this implementation
include that data.

Bulk tab closing groups downloader objections in its confirmation. The reference's
extra "no, but show me the pages" response on an objection dialog is not exposed;
the native question offers yes/no. Other context submenus remain deferred.

- Hash clipboard actions honour the booru prefix option, but do not show the
  reference’s missing-digest warning or its transient hashes-copied notification.

Duplicate and collapse tab actions now work on native page trees; collapse uses
the native default local search domain (my files). The broader reference default
local-location preference is still deferred. An accepted collapse freezes the media shown when its confirmation
opened; if its source keys have left their shared notebook, it does nothing.

A formula editor's import of something that is not one parsing formula (a
page parser, or a package of two definitions) shows "Import one parsing formula
into this editor." where the reference says "That was not a formula--it was a:
<type>". Importing a formula from a clipboard bitmap is not supported; PNG
files and text are.

## Downloader and URL display

The native display editor uses a tab selector and an inline yes/no/cancel question
instead of Qt's notebook and modal question dialog. Native gallery selectors use
an explicit "show other downloaders" switch for the secondary choices. URL display
preferences are a separate durable native setting; the legacy domain manager's
URL-class visibility keys are not imported. Until edited, all installed URL classes
and unmatched URLs are displayed. No site-specific display defaults are installed.
An open media viewer refreshes its links within one second after Apply. Invalid
unmatched URL text retains the reference's "unknown" label but cannot launch the
system browser. URL links are right-aligned in the ratings hover frame; long labels
are elided and the native media viewer does not add the reference's hyperlink
context menu to these links (the existing URLs menu remains available).
The tag-dialog default service and remembering preference are consumed by native
manage-tags windows. Their service tabs are limited to local tag services; the
reference also offers repository tag services. Missing or unsupported saved
services fall back to the first local service by name.
- The file-search options page exposes the default tag service, initial search
  synchronization and `system:everything` visibility; its remaining
  autocomplete and search-limit controls are absent. Tag-editing exposes only
  service memory and the default service; ManageTags currently has local tag
  service tabs, so a repository default falls back to its first local tab.

The File Search initial synchronization and `system:everything` controls now
reach new-page creation and read autocomplete. Hiding the suggestion does not
prevent entering that predicate manually. Other File Search presentation and
location controls are still assessed separately.

Notebook session save dialogs use the existing native text/warning/question
window: reserved-name warnings appear inline rather than as a second Qt warning
window. Append reports a missing destination/session as an error without creating
a notebook; saving a source notebook removed before acceptance reports an error.
Automatic GUI-session lifecycle history and legacy historical snapshot import
remain outside the manual notebook session menu implementation.

The network runtime now exposes request-scoped retry, domain scrub, gallery-token and five-second bandwidth override commands. Recent failures remain available after short requests finish (128 entries per daemon epoch, long text follows the reference's displayed prefix). These commands are being connected to the page controls; the existing current-job review remains separately scoped. See `oracle/fixtures/network_job_control.json`.

The default/fallback local search location is editable and consumed by native
blank-page creation and tag-domain fallback. Its native button opens the current
importable-domain tick list directly; the reference offers single-domain menu
shortcuts before that same multi-domain selector.

Manage Tags' write autocomplete now has storage counts, typed/ideal elevation,
parent and sibling rows, manual fetch, a scrollable suggestions list, multiline
paste and all six Tag Editing autocomplete preferences. Favourites and children now use the shared tabs. Logical multi-selection, batch activation, selected copy/search/relationship menus and local tag-display regeneration work across shared write inputs. Regeneration publishes recalculated display graphs and counts atomically; Qt queues a subsequent display-maintenance pass and shows its database job progress. Native regeneration currently waits on its writer without that job-progress panel.
Declining a multiline paste resumes the native line editor at the retained
cursor/selection. The shared write input uses Slint's native TextInput cursor/selection/IME
with a short multiline allowance during normal paste, retaining Qt's
raw newline draft text instead of standard Slint LineEdit's newline-to-space
conversion. Accepted clipboard tags preserve an existing text draft in both
clients. Undo/Redo uses history owned by each input, keeping a replacement paste
atomic, separating subsequent typing and invalidating redo on a fresh edit.
Cursor movement and Undo/Redo boundaries prevent later typing from merging
into an older command, and Delete and Backspace retain separate directions.
This avoids Slint's separate selection-deletion and insertion undo items. Undo
restores the pre-edit selection; redo restores that edit's saved post-edit
caret, while Qt can retain a later command's caret/selection at a redo boundary.
Same-value programmatic draft resets are not distinguished from live model
refreshes, so this remains a limitation of the partial shared-input assessment.
Platform widget appearance differs. Import additional-tags and whitelist fields now open a detached shared write-tag editor; their raw multiline fields remain available as well. Expanded
parent rows enter their originating child, matching Qt's logical-list selection.

File Search list heights and floating policy reach new-page presentation;
existing pages retain the values captured at construction, as in the reference.
Native list rows use the desktop client's 22-pixel text-row spacing rather than
Qt's platform font-metric size hint. Floating results share their highlighting,
scrolling and selection behavior with embedded results.
Read-list scroll bounds derive directly from the current model's row count;
highlight reveal is reapplied after row-count and viewport-height changes, so a
replaced list cannot retain the previous list's scroll extent. Native regression
source checks the dense selected mask, viewport bounds and rendered first/last
rows through the real selection callbacks. Literal-parent Qt replays show their
native owners before activation and separately assert hidden-owner rejection.

Subscription import-option clipboard commands preserve the reference custom-paste
callback, which replaces directly; the favourites custom-overwrite chooser and
favourites controls remain separate work. The exchange codec accepts all
eight native kinds, upgrades supported old versions through the legacy reader,
and preserves stored external-program definitions. Current and deleted location contexts are both retained during exchange.
Native presentation consumers still assess their domain filtering separately. PNG exchange is available in the typed codec; its subscription UI entry is
assessed separately. These remaining limits keep the broad exchange items partial.

The implicit search limit and explicit-limit sort-refresh controls now reach the
shared search engine and native search pages. Sort-refresh eligibility matches
the reference's supported system sorts and excludes all-known-file searches;
namespace/rating sorts and the other unsupported system sorts only reorder the
current subset. The executor's existing explicit-limit semantics are preserved.

The import tag child uses the shared write-input behavior and detached Apply/
Cancel transaction. Its lists and button layout differ from Qt's input-tags
modal dialog, and the parent still offers its existing raw multiline fields.
Shared write context menus now copy tags/counts/parents, toggle local decorations,
manage favourites/most-used with removal questions and launch new search/duplicate
pages. Relationship lookup and owned seeded sibling/parent editors now work across the
write inputs. Maintenance/admin actions and multiple-tag selection menus remain outstanding.
Per-widget file/tag-domain buttons now work; the multiple/deleted selector
offers the existing native advanced domain ticks outside advanced mode, so it
can preserve any chosen combined domain.

The options favourite-tag list uses a detached shared write-tag editor rather
than the reference page’s embedded list/input. Manual choices and pasted tags
only add; explicit removal deletes. Its accepted list waits for parent Apply,
and cancellation preserves both saved favourites and unrelated options drafts.

The native tab selector is a compact dropdown rather than Qt tab buttons.
Children and favourites use the real service and domain contexts; unknown
favourites remain selectable and zero-count known children remain in the list. Expanded-row viewport height uses native fixed row
pixels rather than Qt's font-metric character height.

Downloader cog actions now reach the daemon through request/epoch-scoped local
IPC; a stale menu cannot act on a replacement request. The automatic policy is
local to a page control, as in Qt, and is not an application setting. Rule edits
use native detached apply/cancel drafts. Native page controls also expose recent
failed-request text through show/copy, although the Python importer sidebars do
not currently call `SetError`; the shared error widget's Python consumer is the
parser fetch owner, whose hook is a separate slice. The native error dialog is a
scrollable window rather than Qt's critical message box. No explicit clear menu
item is added: `ClearError` belongs to the owner, while `ClearNetworkJob` keeps it.

The shared import-options overwrite and clipboard paths are connected to native
importer editors. Their clipboard errors use the native inline error presentation.
Favourite naming and overwrite semantics have reference recordings and model
replays; the favourites popup and durable save/edit/delete GUI remain pending,
so the broad favourites item is not yet complete.
Import-option tag filters now encode the reference whitelist/blacklist polarity
in every supported container field; the extended Qt recording covers both
polarities. This repair does not expand the favourites or overwrite editor scope.

Page-parser network error popups use native Rust failure diagnostics and response
text instead of Python traceback frames. Error ownership, show/copy, completion
retention and clearing on the next example request match the recorded Qt owner.

Raw parsing-data clipboard failures appear inline with **Problem loading!** rather
than opening Qt's critical message box. Paste feedback stays in the native status
line until the next request rather than using a temporary icon notification.
The raw preview matches Python JSON formatting and Unicode clipping; the existing
PyJson limits for duplicate object keys and lone UTF-16 surrogates still apply.
The native viewer's resize-recentering, checkerboard/greenscreen transparency,
and seek-bar height/hidden-height/nub-width preferences have real canvas
consumers. The unchecked transparency preference uses the native viewer's
existing dark canvas colour rather than the reference client's configurable
palette. Checkerboard tiles and greenscreen RGB values match the reference.
The recenter setting controls the native viewer's existing default zoom rules;
the reference's additional per-filetype zoom-lock policies remain separate gaps.
The seek-bar focus requirement now consumes native desktop activity, as described
below. Preview canvas and MPV-specific presentation preferences remain outside
these slices. A hidden-height value of None
hides the native bar completely when the pointer is away; Qt internally retains
a five-pixel ideal rectangle for its hidden widget. The recorder includes both
that rectangle and the actual Qt visibility decision.
File-log clipboard import errors use the log’s native acknowledgement panel
(or the downloader page’s error message for a closed-log menu) rather than Qt’s
clipboard parse-error message sequence. Imports preserve the reference’s first
source type, URL-class normalization, duplicate handling and immediate writes.
Selected-URL searches use the same exact-match OR predicates and local location;
opening the page does not explicitly raise the desktop window.

Login HTTP execution is implemented as a reusable NetEngine consumer with script
editor test controls and result review. Test runs use fresh cookie sessions while
copying request preferences and custom headers. Results now stream into the native list as each step finishes, before the next
wait/request, and can be reviewed while the run is active.
Script Run test now uses the recorded runtime domain prompt and remembered
credentials, preserving old results on either cancellation and clearing them only
at execution start. The script test now shares the native NetworkJobControl over
its isolated engine, including request-scoped cog commands and global bandwidth
rule editors. Explicit owner errors support show/copy/retention; Qt's script
owner does not infer these errors from failed requests. An owned Information/OK
notice precedes the final label update and re-enabling Run, including cancelled
and failed completions. Native notices omit Qt's platform icon/decoration. Help
opens the existing local Markdown documentation; Qt's missing-HTML online/build
guide chooser is recorded but remains Partial, with no native web fallback.
Login script and list parents remain Partial while those documentation and
broader child-editor boundaries remain incomplete. Native copy feedback
stays visible until the review closes. Domain-manager confirmed execution now saves/closes the draft then runs the selected
eligible queue through the shared persisted cookie store. Its progress/cancel
controls appear when the manager is reopened; it does not share the reference's
global login process monitor or automatically log in on ordinary downloader demand.
The manager now shows required-cookie login status and session/earliest-cookie
expiry, refreshes it from the shared store, and resets selected resolved sessions
after the reference confirmation. This reset is immediate and survives parent
Cancel, matching Qt ownership; configuration edits still wait for Apply.
Domain Add/change-script now uses an owned sequential native prompt window for
script, available example/custom domain, access, description and activation, with
the existing credential child. Its final description Cancel keeps the default,
while owner cancellation discards it. Reference no-op separator/current-script
choices, duplicate warnings, validity/delay resets and staged Delete are retained.
Login requests bypass request-admission bandwidth waiting while still accounting
usage and throttling response bodies, using ordinary cookies, custom headers,
redirect and retry behavior.
The executor waits the reference two seconds after successful steps and observes
cancellation during requests and waits. Session-cookie descriptions match the
reference; persistent-cookie result descriptions currently use raw expiry seconds.
Native POST form parameters are sorted on the wire, while the reference preserves
its dictionary order; previews sort them as the reference does. Domain cookie
lookup accepts a port on synthetic local fixtures; the reference cookie lookup
requires the bare domain. Unsupported non-VARIABLE/VETO imported response parser
kinds are preserved but ignored by the login executor. Network/cancellation outcomes
apply the reference four-hour domain delay, guarded by the script key; verification
errors set invalidity and successful login sets validity without altering activation.

The import-options favourites editor uses the native import-options window and
popup presentation. Menu actions, names, prompts, staged parent loading and
immediate profile persistence have reference recordings and native regression
coverage. The earlier pending favourites-popup limitation is resolved for these
editors. Subscription copy-options menus and the standalone defaults manager
still need the same favourites integration before the broad shared feature is
complete. External-program editing remains its separate existing gap.
Historical GUI-session autosaves now run alongside the manual notebook menus.
Legacy historical snapshot import remains deferred.

Startup sessions load before showing the native main window; the reference
defers its initial load by a quarter second. Blank, missing, last and named-session
outcomes, including bad-shutdown recovery choices, match the recorded reference.
Loading an empty saved tree retains the native single blank search page.
Source PNG exports use native SVG fonts and wrapping for their readable header;
the text placement and decorative icon differ from Qt, while the grayscale
carrier/header-height and compressed UTF-8 payload format are compatible. The
native export panel is an owned window rather than a modal Qt panel. Input PNGs
and payloads are bounded to 16 MiB; title and description are each bounded to 4096
characters. Successful exports remember the last directory; a failed write does
not change that preference.

The tags, ratings/locations, and notes hover enable switches and passive
bottom-right zoom/index background switch now have native consumers. Native
hover panels retain their existing layout and contents; their four passive
background copies are now configurable, as described below. The separate focus requirement now
consumes native desktop activity, as described below.
The passive index uses native text styling and palette rather than Qt font
metrics. Its text format, bottom-right three-pixel inset, and placement behind
media follow the reference. Preview and duplicate-filter hover preferences are
separate unclaimed controls.

Write-tag open-search and duplicate-page actions now have a main-window consumer
and real session/query contexts. The optional default-off main-window activation
setting is now imported and staged in Options. A real viewer hover middle-click
creates a single canonical-tag search with its original location and saved default
tag service. Its captured main incarnation and current file/canvas lifetime prevent
retained actions from dispatching through a successor's global launcher. The
owned batch consumer samples the setting once and requests activation for only
its first page, as Qt does. Windows/macOS/X11 use Winit's real focus/raise request
and active-window check; Wayland `focus_window` is unsupported by Winit, so this
control remains Partial with zero completion credit. Native multi-tag selection,
OR/multi-page viewer menus and activation from child manage/write tag lists are
also outside this slice. The new tag producer blocks the viewer's question,
warning, slideshow-period input and owned advanced-delete child; this does not
establish a general native modal policy for other detached dialogs. Headless
request counters do not prove an OS focus grant.

The retained viewer tag action compares a private Store-local file-ID token,
not a content digest. The token, canonical-tag membership and existing owner
guards are all required. The compile/lint API repairs preserve the full recorded
assertions and add zero completion credit; exact-source hosted validation remains
pending.

Application display name remains an existing Partial refinement, with zero new
credit: unchanged Apply now normalizes raw empty imported names, Cancel preserves
them, whitespace remains literal, and title refresh belongs to the current live
main binding. Qt updates QApplication's display name and resets every existing
top-level title; native child-window application identity and platform decoration
integration remain unfinished. The actual offscreen Qt fixture observes unchanged
child `windowTitle`/native title alongside the changed QApplication display name.


Native idle tracking covers input in every desktop window through the event-loop
handler. The reference also polls the operating system's global cursor position;
movement outside native application windows does not yet reset the mouse timer.

API idle activity uses a timestamp-only file shared with the GUI, independent of
the API database lock. The autosave monitor consumes it on its next timer tick;
its resolution is milliseconds. API activity is separate from user and mouse
activity, as in the reference.

An imported named startup session retains both its saved name and the native
live last-session identity. Its named snapshot shares no mutable importer-log
dependency with subsequent startup copies. Other imported saved sessions still
use the existing placeholder conversion for downloader pages; historical legacy
session backups remain deferred.
The native viewer now consumes the duration drag-blocking and drag cursor-hiding
preferences, cursor anchoring/touchscreen override and the independently scoped
idle cursor timer. Drag cursor hiding preserves the reference's blank cursor
after release until another movement; anchoring now uses the saved preference
instead of assuming ordinary unanchored movement. Wider touch and mouse-control
families remain independently scoped. Media Playback's animation start percentage
now stages and imports the old YAML fraction, preserving it on Cancel. Apply
normalizes the spinbox's truncated/clamped0–100 value while retaining concurrent
unrelated changes. Saving preserves the actual spinbox integer: typed29/57/58 save
0.29/0.57/0.58, while reopening truncates those floating values to28/56/57 in Qt.
Unchanged Apply writes the displayed value, and an explicit29 edit over raw0.29
still saves0.29. Fourteen actual staged control cases cover this boundary. Native
WebP/ugoira viewers and the existing archive/duplicate
filters seek their readers before the first decoded frame. As actually observed
in v688, the index uses the previous widget's frame count: fresh/cleared widgets
start at zero, and reused ones use `int((previous_count-1)*fraction)` before
installing the next metadata count. Zero/missing metadata becomes1 afterward.
An out-of-range initial request waits for an explicit seek rather than showing a
clamped wrong frame; stopping releases that blocked reader. Negative/overflow raw
fractions cannot be represented as a native unsigned frame index and therefore
remain non-admitted until explicit seeking or accepted normalization. MPV and Qt's
media-player reference consumers do not read this preference, so no percentage
seek is added to native MPV. Broader player/per-filetype admission and preview
animation controls remain Partial; this does not add GIF/APNG readers. Actual Qt
recording includes paused WebP/ugoira pixels, control staging and42 metadata cases.
Native reader/window regressions and two render artifacts are authored; execution
and image inspection remain hosted-validation work.

The large-session warning uses the native popup stack with the exact reference
text and once-per-boot allowance. Active weight is checked by the one-second
session monitor; the reference checks when its page-count menu becomes dirty.

Login step argument maps now have three independent extended-selection lists, as
in the reference. Native Add/Edit now asks the same sequential key/value text
questions, with remembered edit defaults, key-stage duplicate warnings and
Cancel/blank-key aborts. Blank values are accepted; dictionary precedence and
confirmed deletion remain unchanged. Warnings use the native inline error text. The scrollable native body keeps its footer visible.

Subscription import-options favourites have reference menu actions and overwrite
semantics through the native shared editors. Their warnings use the existing
native information panel, and their popup style follows the native theme. Global
import-options default management and external-program command editing still have
separate incomplete coverage; this does not complete those broader controls.

Login global cookie requirements remain editable through an owned child list;
step cookie requirements now use the reference's embedded list with direct
Add/Edit/Delete and independent selection. Both use immediate sequential
**edit cookie name** and **edit match** dialogs.
Cancel at either stage retains the whole original pair, and edit dialogs preload
the original matchers. The reference also embeds its global script list;
the native script editor still opens that list in an owned child Window. Independent matcher objects with
identical descriptions remain distinct, as in Python. Explicit matcher edits
canonicalize their unused auxiliary matcher values. The three argument-list topology
and selection gap is closed; example-domain add/edit/delete and the reference
default/description-cancellation rules are implemented. The step list's confirmed
bulk deletion, sorted selection, direct matcher routing and owner Cancel are
recorded by `oracle/record_login_step_cookies.py`; script-list topology remains
a separate boundary. This parent workflow improvement receives no leaf credit.
Startup recovery uses the native session-question window and a native GUI running
marker rather than the reference controller's process marker. Choosing blank
keeps the configured startup name for the next boot. Native importer workers
start only after the recovery choice resolves.
Search-log exchange follows the current Qt duplicate dialog's early return for
its “add all urls, even duplicates” button, despite that label suggesting an
import. Invalid URL text remains an unprocessed gallery seed, as in the
reference; clipboard access and malformed PNG carriers get an acknowledgement
window. Native duplicate/continuation questions use owned nonmodal windows
instead of blocking Qt dialogs, and Escape explicitly cancels pending import.
PNG export uses the shared native title/summary/description/width panel and its
bounded grayscale carrier. Complete page-object JSON excludes runtime run tokens
and force-next-page flags, as the reference serialiser does; set ordering is
stable in the native output. Whole-log exchange dialogs belong to the main
window, while an open log owns and closes its own children.
Seek-bar and hover focus requirements now reach the shared winit native activity
observer without installing another backend handler. Hover focus preserves the
reference's transient no-active-window exception for panels already raised,
while suppressing new raises and hiding panels when another application window
is active. Native hover content/layout and the reference's menu/dominant-hover
interaction rules retain their existing differences; the implemented focus
preference applies to all four existing native hover panels. Focus callbacks use
weak viewer handles and do not keep closed components alive.

Example-domain text/access/description questions share one native staged window;
the reference opens separate quick-entry dialogs. Their defaults, duplicate checks,
blank constraints, accepted values and the final-description Cancel behavior match.
The script editor keeps a visible test-domain field instead of prompting on every
run, and test results are delivered at the end of execution. Those presentation and
streaming differences keep the broad script page short of full parity.

Subscription quality review and CSV read durable query queues asynchronously,
with the reference's count and percentage formatting. CSV deliberately uses
raw comma-separated names and has no header or quoting. Native information and
worker errors appear in the edit window's acknowledgement panel rather than a
Qt information/global exception dialog. A disappeared saved queue is reported
as unavailable; the reference DB layer wraps that missing-object error as a
DBException. Closing the native editor stops polling and prevents a late
clipboard publication; cancellation is checked between queue reads.

The network boot pause preference follows the reference’s direct menu toggle rather than introducing an Options control. Its startup helper only sets the live pause when enabled; a disabled preference preserves any existing live pause. GUI startup applies it before daemon startup, and standalone `hydrus serve` applies it before constructing API/download workers. An attached daemon belongs to an already-booted GUI, so restarting it preserves live Resume. The oracle records actual Qt menu triggers and option serialization, then executes the actual ClientController boot conditional in isolation; native tests exercise durable reopen and real loopback network requests.

The Tag Editing service-listbook and three storage-list decoration defaults now
reach Manage Tags; the service navigator also reaches sibling and parent editors.
These are opening defaults independent of write-autocomplete decorations. Native
service tabs and list rows use Slint geometry. Inherited parent ordering follows
natural tag order; Qt's inherited-parent collection does not specify relative
order. Existing Manage Tags differences remain: multiple stored-tag selection and its
full context menu are not implemented, and remote service petition dialogs are outside this
local-service slice. The four preference leaves do not claim those parent
workflow gaps complete.

The default gallery-source Options control checks the installed URL classes,
API conversions and actual parser definitions. As in the reference, nested GUGs
skip missing members and an empty nested source is functional. Native additionally
rejects cyclic imported nesting to keep selection bounded. Category selection
reuses one owned chooser window for the second list; the reference opens a second
quick dialog. Existing gallery-page and subscription source controls retain their
previous presentation; this slice implements the Options default and verifies its
saved value reaches those consumers.

Options' import-options manager uses a child window with three expandable lists
rather than embedding those lists directly in the main Options page. Informational
messages, reset choices and confirmation questions appear within that child;
HTML help opens the online import-options manual. Its default editors inherit
only less-specific contexts, while importer buttons retain their existing lists.
The reference currently raises a tuple-unpack error for single-profile deletion;
native asks the intended named-profile confirmation and safely deletes after Yes,
so that original delete entry remains partial. Resetting profiles refreshes the
native visible list immediately; the reference updates its manager but leaves the
old list visible until reopening. The paste menu preserves the reference's current
label-to-handler mapping (merge-paste invokes replace, fill-in-gaps-paste invokes
merge, replace-paste invokes fill). Existing import-options editor limitations,
including program-command editing/execution, remain as described above.

Closing-focus preferences now preserve the viewer's original page identity and
exit file, including a hidden undoable source. Replacing the session destroys
that weak source; closing its viewer then leaves replacement pages alone, while
the separate debug preference can still activate the weak main window. Native
activation uses winit's desktop focus request instead of Qt's activateWindow;
headless regression observes actual activation attempts without replacing that
request. The reference's separate advanced and debug notifications remain
separate, so enabling both can request activation twice. Existing viewer
shortcut/menu differences remain outside these ordinary close preferences.

Regex matcher favourites use a dedicated “favourites” popup button, containing the same submenu entries and copy/manage behavior as the reference RegexInput’s combined regex button. Existing regex help/components controls remain separate. The clipboard instruction is enabled but copies nothing, matching the actual Qt action. Favourite validity remains advisory, and the shared manager accepts fragments; its Apply persists global choices independently of accepting the enclosing matcher.

The Options favourites editor already supplied sorted CRUD and typed staging.
Its row input now also offers the recorded read-only saved-favourites chooser,
without recursive management or input replacement. All existing native callers
provide saved preferences and owner validity explicitly; the public three-argument
open API retains its supplied-value default. Hidden/retired owners reject child
opening/actions. Regex, shortcut and routing children block each other's launches,
and an open regex child blocks Options Apply and page/search navigation. Empty descriptions are
rejected with the recorded EnterText message, while whitespace and invalid regex
fragments remain permitted. Native row editing combines phrase and description
in one inline panel instead of Qt's sequential dialogs; Cancel discards both.
The reference editor has no import action. Existing help/component menus and
broader regex workflows remain Partial. `regex_options_editor.json` records actual
RegexPanel staging, nested-input saved choices, both row-input cancellations,
duplicate warning, Edit and confirmed extended Delete; authored native replay
reaches the actual saved StringMatchWindow menu and Store reopen. No local Rust
execution or canonical inventory promotion is part of this slice.

Passive tags, file-information, ratings/locations and notes copies now consume
all four background preferences independently of popup and focus settings.
They paint before the media, preserving occlusion and the reference's notes
origin dependency on the top-right copy. Existing native fonts, information-line
content and rating layout remain in use, so the copies follow the native hover
presentation rather than reproducing Qt glyph metrics. Preview-window passive
copies and hover menu/dominance rules remain separate gaps. The already-supported
index background preference is unchanged by these four controls.

The new-page chooser domain checkboxes and their consumers now follow the
reference's two-domain combined-location rule and independent position choices.
Both clients show at most nine file-search entries; extra entries are omitted,
so top placement can keep combined domains or storage available in a large
service registry. The native chooser remains the existing in-window number pad
rather than a separate Qt dialog; this slice adds no petitions-menu behavior.

The sibling connecting string now persists, migrates from legacy options and
reaches native Manage Tags and shared write-tag labels. The related Qt connector
fade and separate connector namespace-colour options remain unimplemented:
native rows still use a single namespace colour for the full label. This text
control does not complete the broader tag-presentation parent.

The subscription failure budget follows the reference's outer exception boundary;
an Error file status alone does not spend the budget. Native FFmpeg-no-output
imports now retain the typed DataMissing identity through the handled-file result.
The native StoreError set has no general Python DBException/DataMissing wrapper;
additional native outer DataMissing producers must preserve that identity when
implemented. Unrelated existing inner import/veto throttle differences remain.

The v688 URL-class links auto-fill button currently finds candidates but applies none: STATICLinkURLClassesAndParsers returns only previously unlinked keys, while EditURLClassLinksPanel applies replacements only among already-linked rows. The actual Qt recorder confirms empty, partly linked and installed cases; the installed case disables the button. Hydrus-rs reproduces that boundary and adds the reference’s real API/redirect pair review, including converter failure handling and readonly sorting. Existing native validation still rejects saving newly authored associations to unresolved API converters. Native parser links retain their existing explicit chooser rather than Qt’s per-row modal selector.

The GUI Pages close-all confirmation preference now uses the native inline
confirmation area for the reference's exact ordinary and notebook questions.
The history-limit control and search-focus-on-switch preference reach the
existing history menu and all five current text-input sidebars. Reference
history menus additionally bold their newest entry; the native menu keeps its
existing common item styling. Out-of-range values supplied to the
native options model are clamped to 1–1000, matching the reference spinboxes
and keeping stored limits inside the visible bounds.

Cursor autohide now acts on native cursor visibility through winit and retains
the same Slint canvas cursor during redraws. Its owner uses the shared desktop
input/focus routes and an owned Slint timer. The pinned Slint 1.18 popup stack
supplies actual open/close/cancel state for viewer menus; synchronous native
popup execution blocks timer dispatch until the menu returns; the actual show
return starts a fresh wait so that elapsed menu time cannot hide the cursor
immediately after closing. Native hover and
volume controls remain eligible for the ordinary pointer instead of hiding it
while their popup content is being used. Backend-specific MPV widget dragging remains a separate gap.


The parser-link chooser follows the recorded Qt order, current selection,
separator no-op, and clear confirmation. Its native detached window has a
single-column table rather than Qt’s list widget; the no-parsers warning appears
in the owner’s error area with the exact reference text. Neither presentation
changes the persisted association or its live downloader consumer.

The no-selection tag computation limit now reaches actual search-page tag rows
and captions, with typed persistence and legacy import. It limits sorted
thumbnail items before collection members are flattened, as Qt does; selected
items remain uncapped. Native software-rendered sidebar regressions replay
recorded zero, nullable and finite limits, including Options Apply/Cancel and
page reopening. Other tag-presentation and tag-list menu differences remain
unchanged; this completes only the original computation-limit preference leaf.

Domain mask mode, match tester, normalized result, subdomain enabled states,
and per-item Add/Edit/Delete queues follow the recorded Qt owner. Native entry
and counted confirmation dialogs reuse SessionDialog. Regex components and
favourites use dedicated buttons rather than the reference’s combined menu;
both copy to the clipboard without inserting into the entry, and favourites
Apply persists globally even if the domain entry is later cancelled. The five URL preview values use native read-only text
controls rather than Qt read-only line edits, with the recorded output values
and invalid-example retention behavior.

The namespace sorting queue opens as an Options-owned child window rather than
an embedded Qt queue. Its text, advanced tag-view and delete questions appear
in that window rather than separate modal dialogs; they retain the reference's
question strings, answers, escaping, cancellation and ordered output. Options
Apply is blocked while the editor is open, and closing Options cancels its draft
and invalidates retained editor callbacks.

Animation loop metadata and the four top-hover zoom-switch policies are now
consumed by native viewer/filter playback. This does not add players for formats
whose backend is absent: mpv-backed playback still requires libmpv, and the
native animation decoder supports WebP and ugoira. GIF/APNG finite loop metadata
is honored by the existing mpv consumer. The configured hover switch is frozen
at viewer construction, matching the reference button callback; reopening a
viewer uses a newly applied choice. Backend error reporting and player-specific
Options controls remain separate gaps.

The two advanced tag-list display defaults now reach real native page sidebars
and media viewers, preserving their opening modes and the reference's raw,
display and independently filtered tag sets. The recorded all-known-tags storage
lists show raw spelling without sibling/parent decorations, and native follows
that boundary. Existing tag-list context-menu switching and richer service-specific
storage-list decorations remain separate gaps. These two dropdown leaves do not
complete the broader Tag Presentation parent.

File sorts now preserve independent tag contexts and use their selected service
for namespace and number-of-tags keys, including fallback sorting and collection
keys. This backend boundary is recorded by `sort_cogs.json`; the Options-owned
cogs now edit both contexts transactionally. Their two-level native popups
use the reference service groups, separators, checks and advanced-view order.
They edit each sort independently and preserve its full saved context metadata.
The page-level sort and collect cogs now reach actual sorting and grouping,
recorded through a real Qt sidebar and media panel in
`sidebar_sort_collect_cogs.json`. Default Collect’s service cog stages the full
independent context through Options Apply/Cancel and reopening. Native cogs use
a shared grouped popup rather than Qt widgets; they open through normal button
activation, and refresh checks before every popup. The two service-menu rows are
excluded menu nodes, not completion-count leaves. This slice claims only the
concrete namespace advanced-display action; broader search/sidebar and Options
sort/collect parents remain Partial. Repository metadata is offered by the menu
without implementing remote repository work.

The sidebar tag-display action is validated within this finite scope at
`9f6397368`. All twelve namespace cases in the existing fixture yield
the same media ordering across display modes within each service. The recording
therefore proves those recorded results and context preservation, without
discriminating the effects of display filters. The native consumer routes single
media and multiple media modes to their respective stored filters. Actual Qt
menu grabs are painted QMenu trees with intercepted popup presentation, so they
do not establish displayed popup placement or physical OS menu behavior. Broader
service/collect/Default Collect menus and sidebar parents gain no credit from
this one-leaf follow-up. Full Linux and fresh independent native review passed.
Evidence (checkpoint `9f6397368`, in git history before 2026-10-08).

Command-palette preferences and snapshot-based provider/queue models are now
present, with fresh Qt recordings. The Options editor now stages and persists these
settings and migrates legacy preferences. The native Ctrl+P window now queries
and launches pages, history, favourites and the supported native main/media menu
actions. Media results carry an action snapshot and refuse to mutate a different
page or changed selection. The palette closes on native focus loss through the
shared focus observer. The calculator now parses the reference's closed numeric
language, including all its callable names (commas remain forbidden by the
reference, so two-argument calls produce no result). Native math functions can
differ in their final floating-point bit, particularly gamma/lgamma and Windows
atanh. Cross-platform fixture assertions permit at most four ULPs or four relative
machine epsilons only for finite nonzero exp/log, trig/inverse-trig, hyperbolic,
erf/erfc and gamma/lgamma outputs, retaining the float result type and sign.
Zeros, special values/infinite inputs, integer results, errors and ordinary
arithmetic display strings remain exact. This tolerance verifies bounded numerical
semantics and does not claim identical platform-independent display strings or
promote the partial calculator. Expressions
requiring huge intermediate powers or conversion of enormous integers to floats
have bounded native evaluation. The native palette uses plain matched text
rather than Qt's rich-text emphasis and result icons; native menus retain their
existing unavailable commands. These boundaries keep the whole-palette entry
partial while its concrete preference and provider-order controls have consumers.

Shared write autocomplete now records and implements result keyboard wrap,
reversible ranges, physical-row page jumps and selected clipboard output. Native
result rows gain focus on click; the editor retains its own selection/clipboard
engine. Shared search autocomplete tabs/OR controls and asynchronous loading
remain separate workflows; this keyboard slice does not promote their parent
coverage entries.

Write-autocomplete result deselection and reversible mouse ranges now share the
reference's add/remove mode, including inherited rows and retained drafts. The
native result list uses a persistent pointer surface across refreshes. The recorded mouse paths
cover displayed rows; continuous dragging outside the viewport is not covered
by this recording.

Viewing-statistic context-menu style and canvas selection now have native Options
controls and real menu/search/sort consumers, backed by
`oracle/fixtures/viewing_statistics_options.json`. The actual media viewer,
archive/delete filter and duplicate filter now own recorded viewing intervals,
with reference minimum/cap controls, filter switches, live policy reads,
cap-before-minimum arithmetic and latest-start preservation across overlapping
canvases. Duration fields retain the reference's float-to-millisecond truncation
at opening/Apply. Native recording commits at interval boundaries rather than
buffering Qt's60-second flush; the explicit Client API count/viewtime path keeps
its existing semantics. Preview statistics imported or supplied by the API can
still participate in the menu/canvas selection alongside native preview intervals.
The new main-page preview displays decoded stills/posters and consumes the separate
preview minimum/maximum fields, with the same cap-before-minimum and duration-times-five
policy. Successful raster presentation accepts the original request timestamp;
unrenderable files and loading placeholders are rejected rather than counted as
views. Qt accepts media before its player/decoder renders. The native canvas does
not yet reproduce preview audio/video playback, embed/external buttons, interactive zoom/pan,
hovers or rating controls. These broader preview parents remain Partial. Owned
page/request generations retire late decoded frames and rebound-window callbacks;
hidden/cleared media and accepted client close finish once. Actual Qt boundary
recording and authored display/store regressions are in `preview_viewing_intervals`. The still/poster decoder uses a lazy
two-worker pool with one coalesced queued request and bounded replies, preserving
the existing held-decode successor behavior. Close drops queued decoder snapshots;
idle workers retain only Weak Store references and active synchronous decodes
release their Store before replying. Native coalescing/timestamp and resource
retirement regressions are authored for hosted CI; no local Rust execution.

Read-search favourites and children now reach actual page predicates, queries,
shared settings and restored contexts. Their selector is a native dropdown
rather than Qt's notebook header. Children fetch synchronously from the native
snapshot; Qt schedules work/publish with stale-domain checks. This slice records
the typing/tab-switch pending state separately from final query results. Read
result multi-selection/context menus, interactive OR construction and advanced
OR input remain distinct gaps, so the search-autocomplete parent stays partial.

Automatic domain login now runs before downloader connection admission and uses
the shared persisted session cookies. Its crash-safe per-store file lease spans independently opened GUI/daemon engines
and real manual/forced login attempts. Owner identity and cancellation are
persisted without a global mutable registry; stale crash metadata is ignored
when the file lease is free. The native domain manager monitors and cancels
this live owner directly instead of showing the reference JobStatus popup.
A triggering downloader cancellation leaves its engine-owned login alive, while
process cancellation stops later steps and records the reference four-hour delay.
Invalid ordinary requests wait 60 seconds; subscriptions retain the exact
reference cancellation note.

Files and Trash confirmation preferences now reach thumbnail and viewer local
file operations. The native deletion question still presents one action rather
than the reference's complete service/action picker when advanced mode is off.
Advanced local-domain, trash and physical/clean deletion now uses the action and
reason picker with the ordered reason queue and remembered accepted choices.
Remote repository/IPFS actions are outside this local dialog. Copy/move-domain
confirmation controls are not yet connected. Undelete currently restores immediately.

Read-search OR construction, rewind and cancel now have native consumers. The empty-OR and advanced Boolean child editors are now implemented.
The shared read autocomplete remains partial: multiple selection, read context
menus and asynchronous fetch publication are assessed separately. The native
rewind/cancel controls use text buttons rather than Qt's icon buttons.

The OR child keeps the opening file/tag domains; it does not yet embed Qt's
domain chooser and favourite-search cog. Shared read selection/context menus and
asynchronous fetch publication remain partial. The advanced editor supports the
existing native system-parser vocabulary, rejects negated system terms as Qt
does, and limits Boolean distribution to 4,096 clauses to avoid an exponential
allocation. System-predicate editors reuse the existing shared native opener;
the OR owner cancels them on close. Native layouts and text controls differ from
Qt's notebook and icon controls. These additions do not promote the complete
read-autocomplete or OR parent workflows.

System selections in main read and basic OR input now preserve activation
Shift and use the same OR construction broadcast as tags. The executed actual
Qt activation recording covers both consumers, accepted recents after outer
Cancel, history-free drafts and real query counts. Authored model/native
regression execution remains pending hosted CI.

Manage Tags deleted-mapping counts and the global show/hide preference now reach
existing local-service panels, including staged changes and persisted reopening.
Other open native owners observe a toggle within 200 ms, rather than Qt's queued
notification. Native uses a labelled show/hide button instead of the reference
eye icon. Repository Manage Tags panels and their petition/pend action choices
remain an inherited gap; this checkpoint does not claim that parent complete.

The native Incremental Tagging child uses the existing local-service Manage Tags
consumer. Repository panels, repository pend/petition choices and parent-level
uncommitted-change confirmation remain separate inherited Manage Tags gaps.
Reference numeric controls and additive per-file behavior are retained; native
rejects out-of-range callback values without changing the preview. Initial-start
inference now uses the reference Unicode15.1 decimal values and ASCII-run numeric
sort keys, including mixed-script digits and skipped negative subtags. The actual
Qt panel fails to open when its initial integer exceeds signed 32-bit range,
and Python rejects raw previews above its configured integer-digit limit. Native
retains a usable value: it clamps large decimal values to 10,000,000 and correctly
reads long leading-zero values. This intentional difference is recorded in
`incremental_number_boundaries.json`; synthetic previews above the 1,024-character
stored-tag limit are injected only into media tag managers, never written to DB.
The ordinary tag-selection parent remains independently
assessed; these two features do not complete it. Adding a fresh tag then removing
it now retains a staged deleted mapping, matching Qt instead of treating that
sequence as an unchanged draft.


Frame locations: the complete imported table and geometry editor persist all
fields, but placement consumers currently use remembered size/position and
maximised/fullscreen for the main window, media viewer and Options window. Existing named dialog owners also use their wired frame keys. Default gravity,
parent/cursor positioning and screen fitting remain incomplete; the broader
frame/table/editor coverage remains Partial.
The real Options lifecycle does not retain incidental resize/move geometry on
Cancel/X or unchanged Apply: the accepted-dialog geometry save occurs before
the GUI page commits its captured frame table. Native retains the same final
Options-frame values, including explicit own-frame resets, as recorded in
`options_geometry_lifecycle.json`.

Service review trash deletion delegates physical unlinking to the existing
deferred-delete worker and preserves its delete-lock behaviour. The recorded
rating warning about restarting media views is retained: review counts reopen
from the database immediately, while cached media ratings refresh through their
existing viewer/page lifecycle. The service review parent remains partial;
this slice covers only the two local bulk-maintenance leaves.


Tag-banner editors use RGBA spin boxes with swatches instead of Qt's alpha-colour
picker, and owned inline namespace questions instead of three separate text-entry
dialogs. The colour-picker and deeper child-window hierarchy remain partial.
Generator colours affect thumbnails only, as in Qt; the viewer title uses its
normal information text colour. Repeated namespace rows retain their existing
summary semantics; Unicode decimal numeric collapse is now supported. Authored
native owner/consumer regressions and rendered PNGs await hosted CI.


Deleted-file-record review uses an owned two-stage inline confirmation in place
of Qt's two modal questions. It retains exact messages/labels and the reference
trash-history exception. Clearing records does not unlink bytes or cancel the
physical-delete queue; it changes future import recognition through the existing
content lifecycle. No broad service-review parent completion is claimed.

Human text sort keys now mirror Qt's two separate steps: split ASCII digit runs,
then convert every complete decimal chunk, including Unicode scripts, to an
integer. Empty chunks and decimal zero share the same tuple. Banner previews
therefore place fullwidth numeric subtags before alphabetic ones while retaining
the reference's mixed ASCII/Unicode chunk ordering; the existing40-state exact
banner replay remains unchanged. A fresh live Qt preview recording covers eight
Unicode/mixed-script/zero boundaries and four sort-key equalities.

The rating configuration example uses the existing native star/counter graphics
and an owned inline counter-value prompt rather than a Qt modal child. Thumbnail
and media-viewer samples use their actual typed sizing preferences. Preview and
dialog samples now read their own saved preferences, also imported from the
reference. Manage Ratings consumes the dialog sizes, integer pixel truncation,
outline scaling and dynamic counter widths. Options exposes all four independent
controls with the actual bounds and two decimal places; Qt's documented icon /
counter ratio clamp is disabled in the reference, so no ratio clamp is applied.
The native preview canvas remains absent, keeping both preview-size leaves
Partial. The Ratings Options style selector and live example panel remain
unimplemented; broader Ratings Options and service parents remain Partial. Named SVG rendering retains the existing
fallback. Numerical examples retain the reference opening click conversion while
star-count changes repaint the stored fraction; the allow-zero checkbox changes
the saved service configuration, without changing the opening preview conversion.
This includes the one-star boundary: the preview keeps its opening nonzero scale,
while the saved one-star configuration separately normalizes to allow zero.
Numerical samples now share one whole-widget hit area, including fraction text
and blank space, with held-left drag input. The opening fraction-side conversion
is retained, matching Qt; dragging outside preserves the last valid value, while
an outside press clears it. These pointer routes have a separate actual Qt replay.
This implements the local example-panel leaf; broader service management and
rating sizing/preferences remain partial.


The browser viewer eye-menu collapse preferences now control the native menu's
real window/hovers/rendering sections, with all eight combinations recorded from
the actual reference menu. Native rows cover window top/frame state and initial
window defaults, passive backgrounds, pop-in focus/enable controls and viewer
checkerboard/greenscreen rendering. The reference's ICC toggle, duplicate-filter
checkerboard and pinned duplicates hover entry are not exposed in this browser
menu. The Linux always-on-top warning label is also absent. Native OS frame/top
requests use Slint's platform window properties; platform support can differ.
The three topology controls are validated within this browser-viewer scope
at `a7ca8166f`; broader hover/button/view-options families remain Partial. No deeper missing
action is presented as a placeholder. Options edits preserve concurrently changed
new-viewer defaults, and stale viewer/Options callbacks cannot save changes.

Eye-menu field toggles read and update their setting inside the same writer
transaction, preserving concurrently changed sibling preferences. Mixed root
separators precede their destination section, matching all eight Qt combinations.
Slint's materialized native context-menu tree is internal: authored regressions
check actual declaration order against recorded roots plus compiled menu data
and real action consumers; they do not claim direct OS-menu introspection.

The validated follow-up covers only those three collapse preferences. It adds
actual reopened checkbox and open-menu evidence while preserving the existing
eight-combination, transaction, ordering and consumer assertions. The actual
Qt rerun is unchanged. Its menu trees are recorded by intercepting `PopupMenu`;
only the Options pane has a Qt image. Native popup images therefore cannot
establish Qt popup pixel parity. Full Linux and fresh independent native review
passed at `a7ca8166f`. Evidence (checkpoint `a7ca8166f`, in git history before 2026-10-08).
Expanded native submenus overlap their parent at the right edge; their contents
remain readable, but the stills do not establish simultaneous parent
highlighting or universal popup placement.

The existing absent actions, Linux label and physical window-
manager limits remain; no broader menu or action completion is claimed.


Notebook alignment, gated tab hiding and middle elision have native consumers;
the broader GUI Pages/navigation/tab families remain partial. The dependency
hierarchy selector uses a fixed 190-pixel column of indented page rows. Its basic
collapse and keyboard behavior is described below; full tree drag/drop, context
menus and broader navigation remain unported. Tab thickness is 28 pixels;
native font metrics and equal overflow budgets differ from Qt's tab allocation,
so the exact retained substring at a given pixel width can differ. Middle fitting
preserves Unicode scalar boundaries, but does not yet preserve combining-grapheme
clusters. Labels and persisted page names are never replaced by fitted text.
Slint's software renderer does not implement item rotation. Vertical tab labels
therefore use SVG text resolved by Slint's own native font context and rasterized
with the appropriate quarter turn, preserving native font size, fitted labels,
selection weight and palette colour. Horizontal tabs retain native Text items.
Native hover text always supplies the cleaned full page name (joined lines,
maximum 256 characters); Qt may retain an empty or older tooltip when its stored
tab text did not change. Small native overflow arrows replace Qt's styled arrows.
Rust behavior/render tests for this slice are authored for hosted CI; locally only
real Qt recordings, formatting, diff checks and cached Slint source compilation
were executed.

Numerical rating examples retain a held Left button through an additional Right
press/release, with Qt's Left-priority press conversion. Delivered in-widget
movement continues after Right release; right-only dragging and released hover
remain inert. Slint 1.18 drops its private mouse grab on every button release,
so leaving the sample after the extra Right release cancels its capture: native
cannot resume that chord drag upon re-entry while Left remains held. Ordinary
single-button captured drags retain the previously recorded outside behavior.
This framework boundary remains explicit; full chord capture parity is not claimed.

The rating-preview evidence follow-up targets all four service-editor examples
for like, numerical and counter services, plus the inline counter input. The
earlier 640×900 export showed only the first example. New captures scroll the
actual owned editor at 640×1000. Full Linux run `37618131717` and fresh
independent rendered review passed for this one leaf.
Evidence (checkpoint `95bbbed2c`, in git history before 2026-10-08). Native examples retain the tested saved 20/15/12/12 icon sizes rather
than the Qt recorder's default sizing. Preview Window is an example label,
not evidence of a live Preview canvas. Captures retain the existing native
action sequence: the first like sample is selected and the first counter is 1;
the Qt final stills show that like sample cleared and the counter at 2.
These are different demonstrated states, not matched pixels. Broader rating
rendering and the cross-edge chord-drag limit remain outside this one-leaf scope.

The global missing-archive-time repair uses an owned native window for Qt's
warning, population-choice dialogs and job popup. Native scan/repair work runs
in the background and revalidates the captured candidates inside a single
content transaction. Cancellation before commit rolls back that transaction;
the reference can keep earlier completed batches. A completed background commit
cannot be undone by closing the window afterward. Existing timestamps, inbox,
unknown imports, the exact February 2022 tracking boundary, and legacy deletion
before import keep the recorded reference behavior (the last is counted but
cannot be filled). Reference repair writes do not refresh an already cached
media timestamp immediately; the recorder verifies the committed SQL and actual
media consumer after restarting. Native refresh reloads its timestamp consumer
on completion. Database file history is assessed separately below.

The native repair window keeps its wrapped explanations in a scroll viewport
above the action buttons, rather than reproducing Qt's separate dialog layouts.
Repair cancellation retains the pending worker result, so a commit that beats
the cancellation request still reports completion and refreshes media. Scan
cancellation remains immediate because it performs no writes. Deterministic
regressions hold the writer or withhold UI timer delivery to cover both orders;
the original reference fixture reproduces exactly on a fresh Qt replay.
Read-only test observation uses a container around each normal Slint button,
preserving the button's minimum/preferred size, stretch and focus behavior.
The narrow 440×480 question fits without scrolling. A shorter 440×360 viewport
tests actual overflow rather than requiring a scrollbar when none is needed.
Full Linux run `37682619326`, fresh reference replay and independent rendered review
passed for this one leaf. Menu, cancellation and ownership checks dispatch
callbacks; warning, population and completion buttons use measured pointer input.
The Qt recording proves questions and outcomes, not matching dialog pixels.
Evidence (checkpoint `2e2a24281`, in git history before 2026-10-08).


Suggested tags have real local-service most-used and recent consumers, per-service
Options list drafts and width/layout controls. Related-tag searching, its weights
and duration controls, and file-lookup scripts are still absent, so suggestion
families and the default-notebook-page leaf remain partial. All four reference
page choices are retained and saved; unavailable choices fall back to the first
available native page, as recorded with those Qt panels disabled. This does not
claim normal Qt Related/File Lookup availability. Opening captures existing
most-used panel availability, matching Qt; updates refresh an already-existing
panel without inventing a new tab. Native service-list editing uses owned child
windows instead of embedding the write input directly in Options.

The recent panel provides real history and add-only activation, but its Clear
question, read-time decay and the full suggestion-list keyboard/context-menu
interactions remain outside this slice. No recent/children/related/global
favourite-list aliases are promoted. Native most-used updates poll persisted
settings every 200 ms instead of Qt's publication subscription. New local
consumers preserve staged tag cancellation and retire their callbacks/timers.

The native notebook tree implements single selection, disclosure, collapse/expand
all and basic Qt cursor/activation keys. Its cursor and expansion state belong to
the live page owner and survive hierarchy edits; they are not a new persisted
session payload. It currently follows the reference's default of retaining child
expansion when a parent collapses. Tree drag/drop, context menus, filters, depth
controls, tree history, cog options, empty-space page creation and optional
collapse-all-descendants behavior remain unported. The native sidebar uses a fixed
190px width, text toolbar buttons and 26px rows rather than Qt's splitter and
configurable tree geometry. Broader navigation/tree families remain Partial;
this slice changes only the original experimental show-tree option proposal.

File history is partial. The native frame has real local-domain and typed tag/
system-predicate query consumers, four sampled series, visibility, count/date
range controls, refit, refresh and asynchronous cancellation/owner guards. It
uses a local-domain dropdown, predicate list and typed input; the reference's
full shared read autocomplete results/favourites/children, tag-service/current/
pending controls, source editor, OR/system children, predicate context menus and
synchronisation controls are not yet embedded here. Its SVG chart compresses
flat runs for painting, shows endpoint dates/counts and uses ISO date text
editors; Qt's 25 rotated date ticks, full count grid/legend, native date picker
and persisted frame geometry remain different. Reversed ranges show a native
validation error and preserve the prior range. The store retains the recorded
sample boundaries (including omitted terminal events and repeated reverse event
timestamps). No completion is claimed for the broad file-history leaf or its
Database parent. Only simple single current local domains are accepted; complex
locations show the reference's simpler-domain message. Query cancellation keeps
the previous chart data internally and hides it until a successful refresh;
retired work cannot publish into a successor frame.

Native history uses one owned worker, a bounded wakeup channel and one replaceable
pending request/result rather than scheduling another thread for every refresh.
Superseded work is cancelled and only the latest live request can publish. The
current search executor still finishes an in-flight query before checking the
cancellation flag; closing drops results and the worker exits afterward without
blocking the UI. Thread startup is fallible and displays a retryable error.

Duplicate-filter background A/B intensity controls and the independent
transparency policy now reach the native duplicate canvas. Saved reference
zero is retained until Options Apply normalises the displayed value; None is
preserved. The adjustment follows QColor's 16-bit HSV rounding, including
black's initial lightness and saturation reduction on overflow. The normal
base is canvas white or the saved active override background. Arbitrary QSS
stylesheet palettes and the QColorDialog topology remain Partial, as
do other duplicate-filter presentation/actions. This slice does not claim
those parent rows. Actual Qt recording ran on 2026-10-05; Rust regression
source was authored but awaits hosted CI (no local Cargo execution).
The tab-wheel option now consumes real wheel input in native notebook bars.
Scroll increments use native 120px steps instead of Qt's scroll-button/tab geometry;
selection and scroll directions agree with the recording. Owned in-window tab
gestures now consume the independent normal/Shift chase and hover-navigation
settings and the disable-page-tab-drag flag. The persistent capture surface
survives nested-row changes, and rejects stale source parents, invalid targets
and moves into a page's descendants. Reordering/transfers preserve original page
keys and ordered media/selection rather than cloning pages.
Tab geometry publication includes the initial frame of rebuilt row items and
keeps key, parent and index together with the measured rectangle. The existing
real-pointer fixture replay also checks live row geometry and pressed identity
after repeated reorders; hosted CI validation remains required.

This consumer does not implement an OS QDrag loop, drag pixmaps/cursors, crossing
windows, tree drag/drop, media/external-file drags, or dropping into an empty
notebook's content body. Consequently the two hover-navigation preference leaves
remain Partial: their page-tab consumer works, but Qt also consumes them for
media/external drags. The broader navigation/drag/drop families remain Partial.
The Qt recorder supplies the source TabBar on synthetic drop events and
intercepts only QDrag.exec_ to observe whether a native source drag is offered;
it does not replace reference decision handlers or claim platform drag behavior.
Native pointer/Shift/Cancel/media/reopen replays and the saved transfer PNG are
authored for hosted CI; no local Cargo builds or tests were run.
The native router rejects captured pointer releases outside the live bar viewport,
including clipped tab rectangles and its reserved arrow area. The Qt overflow
probe records that raw `QTabBar.tabAt` can return a clipped tab index for an
off-viewport coordinate; this is a lookup probe, not proof of OS drop dispatch
there. Native viewport rejection is an explicit capture boundary, with platform
drop routing still outside the claim. Real Slint Move-event replays also retain
the press across `pointer-event(Move)` followed by `moved`, and preserve ordinary
unpressed tooltip hover.

Related-tag weights use native owned drafts and inline namespace/weight question panes rather than Qt modal child frames. The two tables share a native view selector while retaining independent selections and header sorts; Qt displays both lists together. Header order follows Python casefolding with full-row tie-breaking. Native Manage Tags now offers related suggestions for its captured service/files, local/all-known domains and storage/display graphs. Its single coalescing worker computes the full corpus cosine scores, applies the recorded two weight stages and integer truncations, then returns the first 100 base-score candidates. Qt additionally samples large search-tag populations under configurable quick/medium/thorough time budgets; native does not implement those budgets, selected-main-tag search/exclusion mode, alternate tag-context service, repository Manage Tags, or file-lookup scripts. These wider suggested-tab/default-page families remain Partial; importing duration values does not make their controls usable. Default-page fallback excludes unavailable panels and per-service notebook selection is retained within the owner. The recorded binary64 result-weight boundary (100 × 29% truncates to 28) and sibling alias/ideal deduplication are preserved. In display mode, all-known queries intentionally use raw co-occurrences with display totals, matching the reference’s different local/all-known behavior. Numerical floating-point edge behavior beyond the fresh small-corpus ranking fixture remains unverified.

The read autocomplete favourite/children panes now support selected batches and
favourite editing, with a stable pointer surface and actual SearchPage/OR
consumers. Existing favourite/child values are wrapped directly as typed tags;
new entry cleaning remains separate, including its removal of a leading
`system:` prefix. The children consumer intentionally cleans lookup spelling,
as Qt's `GetDescendantsForTags` calls `GetTagId`: literal `system:inbox` supplies
the `inbox` parent chain without changing its active predicate. Actual Qt
`read_tag_tabs.json` covers that boundary, wildcard parents, child removal and
restored negative tags; native/model replay assertions are authored but were
not executed locally. No broader parity count changes. Settings notifications
use one owner-held 100ms revision watcher for visible main read panes and detached
write editors rather than Qt pubsub. Favourite-menu commits advance that revision
just as accepted Options do; plain database writes do not publish an accepted-settings
notification. Count
queries remain synchronous, tab selection uses the native picker, and this
slice does not add the full inherited read-list copy/open/relationship,
decoration/display-mode, drag, or asynchronous child-query menus. Basic OR
children share selection/broadcast but retain their existing context-menu
boundary. Manage Tags' independently owned suggestion/related workers are
unchanged. Broader shared read/write autocomplete parents remain Partial.
The three thumbnail-navigation preferences have real staged/imported/persisted
controls and existing keyboard/wheel consumers. The independently assessed
preview-focus controls receive no additional credit in this batch; the broader
thumbnail family remains Partial. Default selection behavior and its range/ghost invariants use
the existing API; the optional last-hit origin is a separate live path.

The native regular grid implements vertical reveal and wheel scrolling. It uses
Winit's 60px normalization for one line tick and Qt's recorded default of three
wheel lines, with a page cap. Smooth physical-pixel deltas use that same scale;
platform-specific wheel-line settings, Qt's fractional wheel accumulator,
horizontal scrolling and wheel input directly over the native scrollbar remain
outside this slice. Non-finite/out-of-signed-int-range rates retain the previous
step without reproducing Qt's conversion exception; raw accepted rate text stays
saved. Malformed floating-point text silently preserves the previous setting,
including Unicode decimal digits and correctly placed underscores.

The real Qt recorder disables MPV availability only in its private offscreen
process, avoiding a fatal GPU-log callback that blocks initialization. Thumbnail
selection, scroll, Options and rate decision handlers remain unchanged; preview
playback is not recorded. Native real pointer/key/wheel replays and the
`thumbnail-navigation.png` capture are authored for hosted CI. The current
follow-up repairs overlapping narrow sidebar captions without changing the
navigation algorithms or reducing the original assertions. Adaptive paired-button
stacking is native accommodation: the Qt source has horizontal pairs, and the
wider reference image does not establish matching narrow geometry.
The visible search-content minimum now preserves intrinsic height so the existing
ScrollView has a real scroll extent; hidden search content remains zero minimum.
Actual wheel movement and strict viewport containment passed the repaired-source
full Linux regression and fresh four-frame review.
Actual saved Options captures and supported narrow/wider frame checks passed full Linux run
`37644009033` and fresh independent review. No local Cargo builds/tests or
mutation runs were performed. Exactly three navigation preference leaves receive
credit; sidebar, preview-focus siblings and broader parents remain separately
assessed. Evidence (checkpoint `1a49a30f0`, in git history before 2026-10-08).

The namespace-colour Add/Delete and OR-row namespace preferences now have staged
Options controls and real list consumers. Rejected namespace input closes its
Enter Text child and opens an owned Warning notice with the exact Qt message and
an OK acknowledgement. Parent Apply stays blocked until acknowledgement; parent
Cancel closes and retires either child. If a native child cannot open, a dedicated
namespace-list message displays the failure, retaining any rejected-input warning.
The broader namespace-colour editor stays Partial: colour-picker editing,
inherited list menus, and keyboard navigation are not added here. Native active
OR predicates retain their existing single-line layout and use the configured
OR header colour for that row; Qt decorates an OR header plus independently
coloured child rows. Qt's "OR connecting string (on one line)" currently has no
active renderer consumer; this slice does not change that inactive preference.
Authored native replays and namespace-colours-draft.png await hosted CI; no local
Cargo builds, tests or mutation runs were performed.
The namespace colour controls now replay Add sorting during active Shift and Ctrl+Shift ranges, along with enabled, no-op Delete clicks for protected and empty selections. These finite controls do not complete the broader namespace colour editor, colour picker, inherited menus, or keyboard navigation.

Ordinary remembered-window offscreen rescue now consumes the three GUI preferences
at the main and existing named-dialog placement boundary. Native monitor bounds
come from the opening Winit window; the check has owner-local state and preserves
hidden/retired-window boundaries. Winit has no portable available-work-area origin,
so the final fallback uses primary monitor geometry rather than Qt's taskbar/dock
work-area origin. Mixed-DPI global coordinate conventions and compositor-enforced
positions can also differ. Default parent/mouse positions, unkeyed windows, the
self-sizing media-viewer exception and dynamic Qt rescue after later screen/layout
changes remain unported. These limits keep the rescue controls Partial proposals;
exposing them does not complete the broader GUI/frame family. Deterministic
multi-monitor reference cases supply only QApplication screen geometry; the actual
GetSafePosition decision handler is unchanged. Native OS movement and authored
Rust regressions remain pending hosted validation.

Options > open externally now edits registered URL and MIME-specific file-call
queues with owned choosers and a staged nested calls child. Parent page/search
navigation and child launch paths exclude the owned shortcut editor while a
routing child is open, and vice versa. Blocked two-way list selection restores
the editor's current page. The eight finite
Add/Edit/Delete/choose/order controls have executable Qt recordings and authored
native/model persistence/consumer regressions; broader page/list parents remain
Partial. The first/default route reaches main thumbnail and live viewer buttons,
shortcuts and existing default menu actions; missing configured identities produce
owned Information notices, while empty routes use the OS launcher. Specific empty
filetype rows suppress inherited custom routes. Names wash only on parent Apply,
with stable-key deduplication and no same-name remapping of deleted definitions.

The current URL Add/Edit follow-up keeps its physical-button claim limited to
an unscrolled 960×800 scale-1 native view. Chooser answers remain callback-driven;
Qt's existing PNG depicts a later queue state and is not a paired chooser or
notice image. Full Linux run `37599290504` and independent review of six fresh
URL defining states passed, retaining full-state ownership/persistence assertions.
Other routing controls, broader parents and deferred platforms earn no credit
from this follow-up.

The nested File queue follow-up uses callback entry and answers. Its recorded
queue states contain names and callable keys, but no selection flags; selection
expectations come from recorder actions and queue behavior. Displayed names do
not establish native keys: complete saved Routing and Manager comparisons and
the post-Apply consumer provide that evidence. Six fresh native captures
use explicit queue/chooser/notice sizes; the historical Qt outer-panel image is
not a matching nested child image. Full Linux run `37605060836` and fresh independent
rendered review passed for exactly three nested File controls.
Evidence (checkpoint `1e2b2e25f`, in git history before 2026-10-08).
Outer MIME mapping controls, physical nested button coordinates,
universal geometry and deferred platforms receive no completion credit.

The outer MIME mapping Delete prompt now follows Qt simple-delete wording,
“Remove all selected?”, including when only PNG is selected. The nested File
queue still uses its recorded count-based prompt. The bounded outer Add/Edit/
Delete follow-up uses callbacks and explicit capture sizes; historical Qt
images do not show matching chooser, blank-child or confirmation states.
Full Linux run `37613614274` and fresh independent rendered review passed for
exactly MIME mapping Add/Edit/Delete.
Evidence (checkpoint `a6d28f4e4`, in git history before 2026-10-08).
No all-MIME exhaustion, universal geometry, inherited keyboard/column behavior,
broader ownership, parent or deferred-platform completion is claimed.

Run `37609745626` failed the MIME replay's ambiguous GIF setup and an existing
re-shown Options wheel assertion. The test repairs select the intended recorded
MIME code and replace cached geometry readiness with fresh callbacks through a
small resize. The precise cause of the missed wheel remains unproven; the
revised readiness test does not establish unresized hide/show reliability.
All prior behavior assertions remain; repaired full Linux run `37613614274` passed.
The failed source and diagnostic evidence remain separate and earn no credit.

OS calls are regenerated only in the opened routing draft, then persisted on
Apply, preserving registered-call-only transactions. Native regeneration assigns
the correct single-file pipeline; the reference manager's missing-file-OS branch
incorrectly assigns its generated file call the URL pipeline. Native choice
windows use an owned scrollable button list; Qt dialog decorations, MIME column
sorting/persistence, full inherited queue keyboard controls, deeper per-call
menus and legacy executable-manager/routing import remain unported. Registered
process launches run off the GUI thread and discard output, as the existing
bounded executor does; asynchronous spawn/runtime failures show a visible owned
notice if the owner survives. Viewer pause follows successful submission rather
than waiting for a bounded external process to finish. Closing an owner suppresses
late notices without closing an already submitted external program. Native
regression sources and PNG render assertions await hosted CI; no local Cargo,
Rust test or mutation execution is claimed.

The registered external-call Options list now implements ordinary deletion,
warning-aware duplication and both Add Defaults menu routes against executed Qt
list-panel callbacks. Typed callable/process drafts and supported reference
clipboard/JSON/PNG exchange are available, with independently owned children and
parent Apply/Cancel. The broader list/reopen remains Partial: legacy executable
manager settings are not migrated, opening only the registered-call page does not regenerate missing
Default OS launch calls, and reference column-state persistence is absent.
Supported column sorting itself uses the reference casefolded full-tuple tie
break and writes the sorted Options draft.

The separate Add Defaults proof uses the actual native menu only at an
unscrolled 1100×800 scale-1 view. Selector rows highlight selection rather than
using Qt checkboxes; question answers and selector toggles remain callback-driven.
The historical Qt callable-child image is not an outer menu/selector pixel
comparison. Full Linux run `37599290504` and independent review of six fresh
Add Defaults frames passed, retaining the full-state regressions.
Evidence (checkpoint `791b72d19`, in git history before 2026-10-08).

The validated two-leaf checkpoint covers only the registered-call Delete and
Duplicate actions. The actual Qt replay reproduces the selected questions,
ordered duplicate prefix/selection outcomes, key checks and reopened names;
random generated identities prevent whole-JSON byte equality. The reference
scripts question answers and its image depicts a callable child, so it supplies
no outer-list/question pixel or placement comparison. Exact-source full Linux
validation and independent review of all seven fresh defining frames passed.
Add Defaults, broader parents, launching and deferred platforms remain separate.
Evidence (checkpoint `1943b19d9`, in git history before 2026-10-08).
The first run also exposes a test synchronization error: after a declined warning,
the owned question closes immediately while the existing 30 ms timer refreshes
the button-disable flag. A bounded real-render wait preserves the original
non-modal assertion. A strict clone-assignment lint is repaired without a waiver.
The failed run and five available frames are diagnostics, not sign-off.

External-call Add/Edit, import/export and nested process/command controls remain
Partial. Recognized unsupported string-converter steps are preserved in native
registered definitions but rejected by reference export encoding; mixed valid
and wrong-class imports are rejected atomically instead of accepting the valid
prefix, and unusually large imports do not offer the reference override question.
Per-call PNG batches, drop import, input-rule clipboard controls and timeout
minutes controls are not implemented. The native process test runs saved argument vectors with a
single owner-scoped worker, a fallible thread start and bounded wait, discarding
stdout/stderr. Cancellation owns/reaps the direct child only; descendant process
groups, full reference output/error presentation and OS default-launch tests are
not claimed. Ordinary executable arguments use the process API; batch/shell
interpreters retain their own quoting semantics. Harmless owned Unicode fixtures
are authored for hosted CI; no local Rust execution is represented as evidence.

The command-editor checkpoint `6b5ca5ab4` validates only parameter editing and
full-template Copy/Paste within the finite scope below. Actual Qt recording covers
queue/selection behavior and exact clipboard review text, but substitutes dialog
answers and clipboard transport; its panel PNG does not establish nested-dialog
pixel parity or notification placement. The test follow-up targets live Add/Edit,
Delete/Paste review, missing-text notice and timed Copy/Paste feedback captures,
plus retained callbacks against a replacement editor. Broader command/process
parents, executable picker/PATH/launch breadth, output presentation and Qt
PageUp/PageDown/type-ahead/scroll-to-current remain outside the pair. Full Linux
and fresh independent rendered review passed.
Evidence (checkpoint `6b5ca5ab4`, in git history before 2026-10-08).

The first follow-up run passed the command tests but failed an existing
hide/show wheel assertion. That test now waits for measured, stable geometry
and inactive animations without weakening its expected result. Diagnostic images
also showed disabled-looking selected controls and missing CJK glyphs; readiness
assertions/settling and hosted CJK fonts resolve those captured presentation
issues in the successful rerun. All original wheel and command assertions
pass. The earlier evidence did not establish a persistent product failure.
Viewer drag anchoring uses the existing winit window's cursor-position API.
Physical cursor warping depends on the platform/window manager, as Qt's cursor
warping does. Headless native replays assert actual pointer-to-media movement,
live Options updates, Cancel/reopen and stale-owner rejection; the model replay
asserts exact Qt warp requests and the strict 50/51-pixel boundary. They do not
prove that an OS compositor permits the physical warp. Actual Qt was executed
on 2026-10-05 at 01:54:44–46 UTC with synthetic pointer coordinates and captured
warp requests; no local Rust build, test or mutation run was performed. Broader
touch input and the other mouse controls remain independently scoped.

The ordinary browser viewer's hover-tag wheel policy is covered by actual Qt
`viewer_tag_wheel.json`, recorded on 2026-10-05 at 02:14:41–43 UTC. It includes
all four persisted choices, child-scroll consumption, short/long-list edges,
strict delay boundaries, direction changes, media grace and retained/clamped
positions. The unusual Qt direction-reset threshold of 250 seconds is retained.
Native list rows use measured native font height; a Winit line tick scrolls three
rows. Platform wheel-line preferences, Qt's fractional wheel accumulator, full
tag-list selection/context actions and hover panes absent from preview/filter
canvases remain outside this control slice. A real native wheel/navigation/zoom
replay and `viewer-tags-wheel.png` capture are authored for hosted CI; they were
not executed locally. No new broader hover/list/preview claim is proposed.

GUI formatting settings are owned values rather than mutable formatter globals.
ISO and byte precision reach the existing GUI display consumers documented in
`gui-format.json`, with staged Apply/Cancel and reopening preserved. ISO uses
Python's current local offset rather than historical target-date DST; separate
UTC/New York/Berlin recordings cover the distinction and POSIX year-one output.
Time-picker and duplicate-review relative suffixes intentionally retain the
reference force-no-ISO behavior. Existing refresh paths update cached labels;
immediate whole-application broadcast timing is not claimed. Backend-generated
network waits now use the engine’s owned saved formatting; settings reload
wakes existing bandwidth/gallery jobs to refresh their labels while preserving
usage, tokens, deadlines, progress and cancellation. File import size limits,
critical-drive diagnostics and network whole/range over-length errors read
saved byte precision. The real reference backend recording holds time and
supplies scripted wait, response and free-space inputs without modifying its
business handlers. Native regressions cover actual asynchronous waits, loopback
HTTP, file rejection and persisted critical-drive pauses; hosted execution is
pending. These consumers share the core ISO formatter without depending on GUI
models or mutable formatter globals.
Integer-locale, radio Return force and menu-button wheel are outside this slice.

Drag and hover-wheel input now use each viewer's own active/closed lifetime,
rather than the main window's latest-viewer slot. Several visible viewers keep
independent drag/wheel state and read saved preferences on every input, as Qt
does. Applying Options therefore reaches earlier visible viewers; drafts and
Cancel do not. Closing an earlier viewer retires only its own handlers, leaves
the successor active, and retained/re-shown closed handles cannot act. The
focused two-owner native replay is authored for hosted CI, not locally executed.
The sibling connector fade and custom-namespace preferences now have typed import/persistence, staged Options controls and segmented painting in every existing native sibling-annotation consumer. As in Qt, storage-list terms do not permit fades, while write-autocomplete predicate terms do. The broader Tag Presentation and tag-list families remain Partial: this slice does not add sibling decorations to native surfaces that never had them, expand OR rows, or complete list menus/navigation. Native fonts, palettes and clipping follow the existing Slint lists; the reference and hosted native PNGs expose the rendered solid/gradient states. Solid backgrounds start at each run’s left edge; Slint’s default child centering would otherwise extend a later solid run over the connector gradient. The selected PNG is saved before its unchanged gradient assertion so a hosted failure retains paint evidence.

Sibling colour replays also capture a selected collapsed-parent suffix with a different namespace colour. Qt and native extent checks keep its gradient fixed while the viewport grows and preserve the earlier solid ideal colour in the trailing area; native font/padding differences remain bounded.


Options > maintenance and processing now exposes the three independent browsing,
mouse and Client API idle timeouts as minute controls (1–1000) with the reference
ignore choices. The browsing and API thresholds reach the running idle-only
session autosave monitor on its next check, with the strict two-minute boot guard
and strict activity boundaries. The mouse timeout uses the same live gate but
remains Partial because the reference polls the system-wide cursor and native
tracking only observes movement in application windows. This does not implement
high-CPU maintenance scheduling, CPU-busy detection or the broader idle settings.
Ignored controls reopen with a hidden one-minute value, matching the recorded
Qt constructor/multiplier behavior. Unchanged Apply normalizes raw imported
seconds to the displayed floor/bounds (0/59/119 to 60; 60060 to 60000). Cancel
keeps raw seconds. Explicit edits merge independently; implicit normalization
does not overwrite a newer value saved while the dialog was open. Native timing/persistence/Cancel/PNG tests are
authored for hosted CI; only the actual Qt recorder and source checks ran locally.

The external-command argument and template-clipboard controls are replayed against
`external_command.json`: actual Qt CRUD/reordering and current-row/Shift behavior,
list keyboard copy/delete, six reverse-selection/current/anchor edge histories,
exact parameter text prompts, and ten split-space
paste inputs accepted or declined. The native command draft keeps raw pasted rows
until acceptance while its example/copy and saved arguments use the reference
cleaning. Clipboard access errors are explicitly owned and do not alter the draft.
Qt's platform-specific list PageUp/PageDown, type-ahead/scroll-to-current details,
physical tooltip placement, executable picker/PATH and complete command-dialog
geometry remain outside these two control leaves; the broad command parent stays
Partial. Native real key/owner/store regressions and the command PNG are authored
for hosted CI, without local Rust execution or mutation testing.



## Metadata filesystem workers

Manage Times and Force Filetype now dispatch finite, GUI-owned asynchronous
work on captured targets. The actual Qt recording in
`oracle/fixtures/metadata_file_jobs.json` covers nine worker inputs plus two
rejected dialogs, real temporary files, strict delayed timestamp popup
publication, file/64-file cancellation boundaries, normal finish/dismiss and
copy-fallback cleanup scheduling. Authored native regressions retain actual
DB/filesystem consumers while waiting for asynchronous completion, and protect
retired parent/date-child callbacks, successor slots and changed selections.
The store replay compares recorded durable prefixes and popup publication,
uses real persisted popup cancellation, preserves timestamp DB updates after
disk cancellation, and checks cleanup queue eligibility at +3600/+3601.

`audit-media-times-disk` and `audit-media-force-rename` remain Partial, with
zero completion promotions. Windows locked files, hardlink/same-file
destinations, cross-device/read-only overwrite, metadata/permission recovery,
full prefix-lock/concurrent storage-relocation behavior and reference exception
presentation need further evidence. Native per-hash media claims and copy-only
shared imports protect existing ownership rules. Copy fallback uses the existing
DeleteNeighbourDupes backend, but the new tests do not execute its maintenance
runner or prove unavailable-file retries. Native errors finish the active job
and publish an error popup; fallible worker-spawn errors currently reach the
caller/log. Broad metadata and generic job/shutdown lifecycle families remain
Partial. Rust regressions are authored for hosted CI and were not run locally.

Importing work slots use runner-owned RAII counters rather than mutable global
controller counters. All five exact controls have typed legacy migration and
live queue consumers, including independent watcher check/file loops. Native
pending jobs re-read saved settings and owner/paused state within one second;
page closure cancels its network job within 250 ms, retaining the permit through
ordinary cleanup. A current blocking local import may finish before releasing
its slot. Invalid saved capacities have effective runtime bounds of 1–500;
raw integer storage is preserved until an Options Apply normalizes the controls.
Per-kind limits and queued-job admission are separate from network semaphores
and subscription concurrency. The broader importing/page families remain
Partial, and no parent is proposed complete. Actual Qt options/controller
boundary recording and screenshot inspection ran locally; the authored
model/native/live-downloader regressions await hosted CI. No local Cargo build,
Rust test or mutation run was performed for this slice.
Monotonic persisted queue identities prevent retired workers from adopting a
successor after SQLite reuses a deleted maximum row. Store opening persists any
pre-upgrade maximum before a runner can own/delete it; initialized stores retain
their saved high-water mark without rewriting it on every reopen. Late seed
updates also qualify the seed by its queue. Pending-pause, deleted-owner replacement and
retained Options callbacks after reopening are covered by authored regressions.
FIFO fairness and background repeater scheduling cadence remain outside this
five-control slice.

The two global viewing-statistics cleanup actions now use owned confirmation and
completion notices. Culling preserves the reference's minimum-before-maximum
order, separate media/preview rules, zero/None limits and invalid-bound errors.
Native SQL writes are atomic and reject time bounds outside its integer range;
errors are shown in the owned warning notice instead of a global Python traceback.
The preview rules also feed the owned timed still/poster display consumer described
above; cleanup remains a separate action rather than the source of timed views.
Broader database maintenance and viewing-statistics families remain Partial.
Actual Qt handlers and the reference SQLite module were recorded on eight cases;
native/model regressions and a question PNG are authored for hosted CI. No local
Cargo build, Rust test or mutation run was performed.

The per-service already-exists filter action uses an owned detached shared tag
filter editor, while Qt opens its modal editor from the cog menu. Accepting
enables the test and stages the captured service's filter; Cancel and owner
retirement do not save it. The importer already stored and consumed this filter,
and now has a native editing route. Actual Qt recording covers real modal
Cancel/accept, saved typed-object reopening, and real per-service mapping reads
for parsed/additional tags, including disabled-test bypass. Authored model,
native and real local-file-importer regressions await hosted CI; no local Cargo
build, Rust test or mutation run was performed. This closes only the original
`import-existing-tags-filter` action; broader service/import-option parents
remain Partial, including other unimplemented controls.

The four original Files and Trash removal controls now have staged typed settings,
legacy migration and live archive/delete-filter, thumbnail/viewer deletion and
thumbnail strict/merge move consumers. Removal is a page effect independent of
physical storage changes. Default false retains trashed rows; enabled trash removal
intersects actual current trash membership and excludes the one-domain trash view.
Filter removal includes keep/delete decisions, optionally skips, and returns focus
to the surviving skipped file. Page identity is captured before asynchronous user
answers, so switching or closing a page cannot redirect a removal to its successor.
Hidden/retired filters and retired viewers reject removal callbacks. Existing
physical migration semantics are unchanged. General out-of-process/API content
updates are not yet broadcast to all native open pages; these proposals cover the
reachable owned native actions, and the broader Files and Trash parent remains
Partial. Actual Qt recordings execute filter close and MediaList pruning; authored
model/store/native regressions await hosted CI, with no local Cargo/Rust/mutation
execution.

Local transfer confirmation parity is limited to the two Files and Trash
checkboxes and thumbnail local-domain add/strict/merge commands with explicit
sources. The broader locations and Files and Trash parents remain Partial:
remote locations, viewer locations, shortcut source selection and importer
transfers are outside this slice. The four removal-from-view preferences are
covered in a separate finite Options slice. Native transfers use one
transaction and revalidate live memberships/service keys after confirmation;
the reference schedules a block worker using captured media. An unavailable
source is skipped, a replaced service is rejected, and a strict destination that
became current is left alone. Large transfer jobs currently execute synchronously
on the GUI thread, without the reference's progress/cancellation worker.

The copy question preserves the reference's count before its final local filter:
a selected trash-only file can contribute to the question while never reaching
the migration. Merge preserves an already-current destination's time; restored
destination memberships keep their recorded original import time. The evidence
packet proposes only the two original Missing Options leaves. Authored Rust and
native regressions await hosted CI; no local Cargo build, Rust test or mutation
run was performed for this slice.

Tag and rating shortcuts: only local tag services take tags (no pending or
petitioning to repositories); the tag is typed rather than chosen with an
autocomplete, and the rating typed rather than clicked on a rating control.
They run in the media viewer only (not on thumbnails), from the "media" and
"media_viewer" sets and the custom sets turned on in the viewer. The
interactive "popup ... entry dialog" commands and file domain commands
aren't offered.

Options > shortcuts' set lists: the command editor still offers only the
commands with a native executor (three for the main window, six for the
viewers), so editing a default binding for another command replaces it with
one of those, and only those run. Default bindings with data are shown, not
run. The viewer's "edit shortcuts" opens the whole options dialog rather
than a shortcuts dialog of its own. A custom set can't take a built-in set's name (the reference only
keeps custom names apart from each other). "restore defaults" chooses from
buttons rather than a list, and the help shows in a message window.

Shortcut capture now has an owned Options > set > command path and persisted
keyboard consumers in the main GUI and media viewer. The two capture policies
migrate from typed legacy booleans. Legacy shortcut sets remain retained as raw
legacy objects; this slice does not decode those bindings or implement the full
command catalogue, custom named sets, multi-selection or shortcut exchange.
Only the original keyboard capture leaf is proposed FirstPass, contingent on
hosted native/model/Store validation. Parent shortcut workflows remain Partial.

The mouse capture leaf remains Partial/0. The reference uses native Qt double
click events and vertical angleDelta; Winit supplies ordinary button events and
line/pixel wheel deltas. Capture uses a 400 ms/five-logical-pixel double-click
fallback, while the viewer uses five physical pixels. Lines map to 120 angle
units and pixel Y is borrowed directly. Multi-button masks, platform task-button
mapping, trackpad angle semantics, global cross-viewer wheel state and native
platform double-click thresholds are not established by the offscreen Qt
recording. Authored Rust tests await hosted CI; no local Cargo build, Rust test
or mutation run was performed.

Shortcut execution now uses permanent owner guards in addition to visibility:
viewer routes read their existing canvas tracker; the main route retires only
through the client’s accepted-exit hook. Exit confirmation, Cancel and its
auto-yes path remain owned by client_exit. Native regressions cover retained
close/show, independent successor dispatch and untouched F7/default command
Apply, with hosted execution pending. This adds no original leaf proposal.

Exit confirmation now checks the main binding's permanent activity and visibility
before asking, timing out or running its retained close continuation. Cancel
preserves the owner; accepted close remains terminal after re-show. Normal rebind
already drops its prior exit timer references; the added guard also protects a
retained timer or close continuation. Native regressions cover a retained timer,
full main-window rebind, hidden acceptance and accepted close/re-show. Hosted
execution remains pending; this adds no original leaf proposal.

The main menu's page-change callback slot now holds a weak reference to its
title callback: the title callback owns menu hooks which own that same slot.
Live window callbacks and the menu timer retain the title callback, while the
weak back-edge permits pages, rows and workers to retire with their owners.
The refresh callback also refers weakly to the duplicates sidebar, whose
file/viewer launch callbacks reach that same refresh callback. The window's
duplicates-action callback owns the sidebar while the window is live; an
ordinary search window installs these launchers too.
Native regressions assert release before thread exit and retain the existing
two-second Store-release deadline. Hosted execution is pending; no local Rust
build or test was run. This ownership repair adds no original leaf proposal.

The three original thumbnail-cache controls now govern the real owned native grid:
saved byte/unit and timeout preferences plus Help/debug clear. The reference can
compress stored bitmap bytes; native accounting measures its decoded RGB/RGBA
pixel buffers instead. Missing thumbnails use a bounded 128-byte negative entry.
The byte limit covers cache-owned buffers, not images still held by the renderer,
loader queues, or icon/tag metadata. A cache is shared across pages of one main
GUI incarnation; separate main windows own separate caches rather than sharing
the reference process-wide cache. Image, tile, prefetch and video cache controls,
other debug actions and their parents remain Partial. Reference soft insertion,
last-access expiry and immediate saved-policy maintenance are preserved. Native
regressions are authored for hosted CI; no local Cargo/Rust/mutation runs occurred.

Page sidebar/preview splitters and the four Pages > sidebar actions now have real
consumers. Drag tokens capture a page and a monotonic window incarnation, so a held
old press cannot resize a successor binding. Live sizes are associated with both
PageKey and SearchPage owner identity: closed/reopened live pages retain geometry,
while fresh session pages use global defaults. Session serialization/cache APIs
remain unchanged. Signed YAML positions and the separate ClientOptions exit-save
switch import into typed preferences; the historical positive-vpos calculation
uses horizontal total minus vpos, as recorded in the actual Qt probe.
The native still/poster layout reserves 80px for the other pane and uses six-pixel
handles with a scrollable sidebar; Qt's intrinsic minimum sizes and four-pixel
handles differ. Actual Qt SaveNow retains the hidden inner preview size, including
an unsaved resize, and saves hidden sidebar width zero. These boundaries reach the
native menu and accepted-exit consumer. Root integration must retain the newer
accepted-close incarnation guards and weak AfterChange callback ownership.

The editable hide-preview setting remains a scoped Partial action. Typed staging,
persistence and new/reset layouts are implemented, and successfully rendered
previews now retain independent accepted file identity, interval and bounded frame
snapshots per live PageKey/SearchPage incarnation. Actual Qt A→B→A, whole-window
hide/show and live close/unclose keep each accepted canvas and interval while the
global flag rejects SetMedia/clear. Native follows those successful-render boundaries;
normal hide with a visible splitter finishes and normal show restores a successor
interval. A previously collapsed splitter independently rejects clear/update,
including a page switch after the global flag is disabled. Its one shared
worker pool remains two workers plus one queued request, with logical retry for a
displaced or normally suspended unaccepted request. Normal return resets its pending
start; displacement under global hide retains the original request time. Rendering
snapshots have a 64MiB soft LRU bound plus the currently shown oversized frame;
evicted snapshots re-decode their accepted identity without counting another view.
A failed restoration remains blank until another page-show/request, rather than
repeating decode every refresh. Broader image-cache controls are still Partial.

The remaining hide-control boundary is explicit in `hidden_page_preview.json`:
actual Qt accepts current file/start before its first raster is ready. Native keeps
the previously reviewed successful-raster gate, so a cancelled or failed first decode
has no accepted file/view, and normal hide before that decode finishes has no view to
save. Cache eviction may also require a loading/blank reconstruction frame instead
of Qt's resident per-page renderer. Existing failed/cancelled decode regressions stay
intact. Native membership/accepted-close retirement finishes successful trackers once,
even if a callback retains a stale SearchPage Rc; actual Qt CleanBeforeDestroy while
globally hidden can refuse its clear, as the recorded accepted cleanup probe shows.
Whole-window suspension without the global hide flag retains the native's existing
finish/clear policy. These renderer/lifetime differences prevent full completion
credit for the original hide control. Broader sidebar/search, preview playback,
hover/rating and structural Options parents remain Partial. No canonical inventory
status or overnight completed ledger is edited here.


The four thumbnail modifier-preview preferences now have typed legacy import,
staged native Options controls and live Ctrl/Shift click and Shift-key consumers.
Each duration-only child retains its value while disabled. The existing ghost
navigation preference is disabled when Shift preview focus applies to every
file, retaining its saved value. Eligibility uses the clicked media's duration:
a singleton `Some(0)` has duration, while Qt's zero-sum collection has none;
a collection's static preview representative does not bypass a timed member.
Plain clicks and anchorless Shift fallback still focus normally. Rejected modifier
targets retain the current preview, without reloading its pixels or restarting
viewing time. The existing owned preview decode/statistics implementation remains
the consumer; this adds no animation, fade, blurhash or renderer-family parity.
Actual Qt recordings cover all sixteen Boolean policies and 192 selection steps,
plus five private real-media duration shapes. Native rendering and Rust regressions
are authored for hosted execution; no local Cargo or Rust tests were run.

Open Externally routing and namespace colours now have distinct native Options
row kinds. Their former shared value rendered both unrelated editors on each
page. Existing native workflows assert the correct page family and keep their
routing and namespace render captures for hosted review. This integration repair
adds no original leaf proposal; runtime and rendered verification remain pending.

The two original Manage Tags default-sort Options controls now reach separate
search-selection and current-viewer-file consumers. Settings import legacy
presentation contexts 1 and 3 independently of sidebar/viewer tag-display sorts;
staged saves merge only the changed context. Native local service tabs capture
opening defaults, remember separate text/count orders, and preserve local sorts
across subsequent default changes. Sorts use sibling ideals when sibling
information is enabled, without replacing the logical stored tags or their
existing segmented colours. Broader Manage Tags transactions, repository tag
services, immediate viewer commits, and tag-sort parent families remain Partial;
this slice signs off only the two default-sort controls. Model, import and native
regressions passed full Linux validation at `7c74c6171`; no mutation runs occurred.

The validated four-leaf follow-up repairs the shared Options row allocation for
the two Manage Tags defaults and namespace-grouping Add/Edit. Native sort choices
remain vertically stacked, unlike Qt's horizontal controls. Actual geometry
regressions check dropdown separation, containment and the following heading at
normal and larger viewports; the page retains its natural scrolling. Reference
parent images show initial defaults while saved native captures show applied
values. Separate consumer captures have recorded behavior support but no matching
Qt consumer PNG. The actual Qt rerun is unchanged; full exact-source Linux and
fresh independent native review passed for exactly the four selected leaves.
Evidence (checkpoint `7c74c6171`, in git history before 2026-10-08).
At 900px, some namespace-grouping explanatory prose clips horizontally; the
selected captions, dropdowns and scrolled queue controls remain readable. The
internal queue viewport shows only part of the stored raw list at once; complete
raw-value equality is covered by passing assertions.

The four experimental download-page update preferences now reach owned native
gallery/watcher list status and sorting. This throttles presentation reads rather
than importer/network work. The reference formula samples displayed items before
refresh, uses `max(minimum_ms / 1000, items / denominator)`, falls back to one second
for a zero denominator, and updates only after the pending deadline has passed.
Reference JSON fixtures use round-trip float parsing: the default decimal parser
could shift a recorded deadline by one ULP. The deadline formula and exact replay
assertions are unchanged.
Saved changes affect the next period without resetting that deadline; explicit
refresh resets it to zero. The native uses the existing current-page identity,
a weak window and permanent binding retirement instead of Qt sidebar objects;
there is no added worker or asynchronous list completion. Highlighted jobs and
independent aggregate/close consumers still read live Store state. Render-time
reentrant page reads defer the list tick without consuming its deadline; a
pending explicit refresh uses weak page identity and cannot force a successor
page. Hosted timer/held-borrow regressions remain unexecuted locally. Four original
Missing controls are proposed conditionally on hosted native/model/Store execution
and render review. Other speed/memory, download pages and Options parents remain
Partial; no canonical status or parent credit changes. No local Cargo/Rust or
mutation tests were run.

The finite one-line OR connecting-string editor now stages and saves the raw
reference `or_connector` value (factory “ OR ”), including blank, whitespace,
Unicode and programmatically loaded newline content. New legacy imports convert
the value; previously imported stores read their retained ClientOptions until a
native override exists. The real Qt renderer's custom-connector loop is inside a
disabled triple-quoted block: its active predicate list keeps “OR:” and coloured
member rows, and ToString/copy/export still use literal “ OR ”. Native preserves
its current one-line OR labels, canonical predicates and separate namespace/OR
colours for every edited value; it does not activate the reference's dormant
renderer. The actual Qt recording covers save, Cancel, serialization/reopen,
labels, member/header colours and default/collapsed copy output. This signs off
only the original editor control; broader OR list layout and renderer families
remain Partial. The original author packet's pending validation is superseded by
the exact-source Linux checkpoint below.

The OR editor regression adds settled reopened ASCII/fox frames and physical
Select All/Copy with a clipboard sentinel at one supported 900×900 viewport.
The Linux system selects bitmap-only Noto Color Emoji, while Slint 1.18.1's
software renderer requests outlines. Startup now prepends bundled SIL-licensed
Noto Emoji outlines before the platform's preferred emoji face in the actual
sans-serif/system-UI text fallback chains, retaining their preceding fonts and
implicit system tails. An emoji-only mapping was insufficient because these
text chains reached the bitmap font first. Fontique repeats the retained prefix
in its implicit tail for unsupported characters; repeated setup does not grow it.
Explicitly requested bitmap fonts or configured bitmap primaries can still
outrank the fallback. Emoji are monochrome and need not match Qt's glyph shape
or colour. The adapter uses Slint's internal font context, so runtime,
build helper and core are pinned together at 1.18.1. A font-selection regression
checks the actual text-control chain's fox font, unchanged representative
Latin/CJK/symbol glyph choices and unchanged script fallback lists; the
real reopened field must paint ink before selection. The fresh Qt replay remains
byte-identical. Full Linux validation and independent fresh image review pass at
`88e9851e6`: all 2,198 workspace tests, including 714 GUI and 67 media tests.
The stronger font-selection test rejects the earlier Emoji-only adapter; its
failed full run and blank field remain diagnostic history. The local cached-library
probe is separate from current application validation. Exactly one original leaf
is published, bringing the ledger to 375 with no existing candidate pending;
there is no parent credit or custom OR renderer activation. Windows/macOS remain
deferred. Evidence (checkpoint `88e9851e6`, in git history before 2026-10-08).

Default and registered single-file launch dispatch now reads current Store
file-domain membership explicitly. The lightweight basic metadata reader leaves
locations unloaded; treating that empty field as authoritative had rejected every
local file before OS/process dispatch. The corrected boundary still rejects deleted
files with retained bytes and accepts restored membership after Store reopen.
Existing exact OS launch vectors and registered process deadlines are unchanged;
a focused native deletion/restoration/retirement regression is authored for hosted
CI. No local Cargo/Rust/mutation runs or additional completion credit are claimed.

Delayed file-view removal callbacks now retain a weak source-page handle instead
of keeping a replaced panel alive through its viewer. They still verify active
binding and open-page identity before removing rows, including externally retained
forgotten pages; they never redirect to the successor current page. Native replay
fixtures that assert trash pruning explicitly enable the saved removal policy,
whose reference default is false for both page and viewer lists. Exit cases start
a fresh binding after accepted close, and advanced deletion emits from a visible
main owner. Every existing assertion, count and deadline is preserved, with an
additional real pending-viewer/session-replacement regression authored. Hosted
execution remains pending; no local Rust/Cargo runs or new completion credit.

The finite Media Playback embedded-ICC switch now has typed saved/imported
state and actual decode consumers. Ignoring an embedded profile still follows
Qt's separate PNG gamma/chromaticity fallback. A saved policy change refreshes
accepted static rasters and sharp tiles without resetting zoom, focus or viewing
intervals; asynchronous replies carry their admission policy/generation. The
native owner watchers poll saved settings rather than using Qt's global cache
publication bus. Stored reference/native thumbnails are profile-free, so the
switch does not regenerate previously stored thumbnails or rewrite durable pixel
hashes. Future imports and explicit maintenance conversions use the live setting.
Native ugoira/WebP players preserve paused frames and use the current policy for
future conversions; this does not promise cancellation of prefetched animation
frames. The reference's static image-cache notifications do not subscribe its
animation widget either.

The recording covers deterministic embedded PNG/JPEG/WebP, PNG gamma and combined
embedded-plus-gamma fallbacks, ordinary PNG/JPEG/GIF, composed animated WebP frames
and profile-free thumbnail bytes. Existing format/plugin and mpv limitations
remain: this control does not establish universal image-format/renderer parity.
The browser eye menu still lacks its separate ICC action. Broader media rendering
and playback parents remain Partial. Native regression/render source is authored
for hosted validation; local checks used the actual Qt recorder, Python/source
invariants, direct rustfmt and diff inspection only.

“Allow loading of truncated images” remains Missing, with no inert native
checkbox. Actual PIL accepts the authored short PNG/GIF/JPEG fixtures when enabled
and reports damaged/truncated-file errors when disabled. The native PNG/GIF and
other format decoders do not yet implement that shared permissive policy; a saved
Boolean alone would not provide its real consumer behavior. The scoped evidence
records this gap with zero completion credit.

The selected collapsed-sibling fade probe measures pixels inside the actual
painted tag-text rectangle. The outer selection strips use the palette blue and
also matched the old colour-only gradient classifier. Exact hosted 760px and
1100px snapshots both show the suffix gradient at x306–377; only the selection
strip moved with the viewport. The unchanged fade-extent equality and trailing
solid-colour assertions still verify rendering. Production paint is unchanged;
cached Slint/static checks are separate from pending hosted Rust execution.

The overflow-tab pointer replay waits for its actual strip and tab measurements
to match the current viewport and wheel offset before delivering input. Headless
paint pumps timers before layout; hit rectangles are published on a following
1ms timer, so one render does not guarantee current geometry. The bounded wait
keeps the exact full-name hover, target-key, clipped-target and captured-drag
assertions, and leaves production pointer routing and paint unchanged. Hosted
execution must validate this replay correction; no local Rust tests were run.


The original File > open > quick export directory action now has a native command
and an owned saved-directory consumer. It shares ExportSettings with the existing
Default export directory editor/manual export consumer; no second last-used path
or new preference is introduced. Configured missing paths are opened without a
validation dialog or creation. On non-Windows platforms, reference legacy
backslash recovery only applies when the original spelling does not exist.
The fallback is created only for an unset preference. Home lookup is owner-local:
POSIX HOME (including an explicit empty value)/account fallback and Windows
USERPROFILE/HOMEDRIVE+HOMEPATH follow
Python expanduser rules without changing process environment. The POSIX resolver
uses the standard-library home API with a local deprecation allowance; its
Windows ambiguity does not apply to that cfg-limited branch. An unavailable home
reports the reference text. Missing
home and creation/read errors use the existing popup queue; OS opener failure
handling remains the existing platform launch behavior. This menu action has no
Cancel dialog of its own; recorded Options Cancel preserves the saved consumer
path. Hidden/rebound/accepted-close owners and resolver-induced hiding reject the
launch and fallback creation. Broader File/database menu families remain Partial.
Rust/native render regressions are authored for hosted validation; local checks
used actual Qt, Python/source invariants, rustfmt and diff inspection only.

The native legacy **colours** controls and Help **darkmode** action affect the thirteen represented painted roles rather than switching the application style or OS theme, matching Qt's legacy override policy. Native RGB picking uses three bounded channels and a swatch with OK/Cancel; Qt QColorDialog additionally offers HSV/HTML, palette history and its platform picker. The structural coloursets family retains this topology boundary and earns no concrete-leaf credit. Generic controls keep the native application palette; Qt QSS support and whole-platform palette editing are separate unfinished style work. Synthetic preview status text remains native status UI, without claiming a Qt canvas text counterpart. Saved role propagation is owner-local and bounded to 250ms; no process-global mutable colour preferences are introduced.

The preview default-zoom control now reaches existing accepted still/poster geometry, including per-filetype preview scale rules, DPR and centered overflow clipping. It does not add interactive preview zoom/pan or player/embed controls. Qt chooses media before decoder completion; native geometry is sampled only after a successful accepted raster and retains the original request timestamp. Saving a choice alone preserves the current image, while actual pane resize reads the new default; unrelated Options and ICC/cache replacements do not reset it. Native resize follows the current preview default; the separate viewer zoom-lock controls still apply to full viewers. Actual Qt geometry/control evidence is recorded; native/model tests and exported PNG review remain pending hosted CI. Broader preview/media-playback families retain their existing Partial boundaries and earn no parent credit.

Thumbnail appearance: saved blurhash fallback and the editable/browsable
background path now reach real native loader and viewport consumers. Existing
imported stores recover their retained reference preferences if the new native
key is absent; an edited native value wins. Background decoding uses Slint's
image formats and the native file dialog, with a path-keyed owner cache matching
the recorded cache behavior. Both reference renderers inherit/use the fixed
viewport background; it is not conditional on the renderer preference.

Fade and the new-rendering-tech checkbox remain Partial, with zero completion
credit. The native page admits a creation-time paint policy and the full
decorated cell participates in its transition, but Slint's software renderer
multiplies primitive opacity rather than blending Qt's already-painted QPixmap.
The old-mode cumulative alpha is an effective-opacity approximation, including
its frame-count accumulation; repeated Qt integer pixel compositing is not
reproduced. Native virtual rows do not implement Qt's manual canvas pages or
QGraphicsScene layout/performance engine. The owned snapshots and lightweight
cache-admission identities do preserve decoded-image/selection transitions,
interruption, cache eviction/reset, cached scroll revisits and GUI retirement.

This slice proposes only the two concrete blurhash/background leaves for
FirstPass after exact hosted execution and native rendered inspection; it adds
no completion credit for fade, renderer tech, parents, aliases or cache breadth.
The new native tests/three native PNG artifacts are authored and unexecuted here;
no local Cargo/Rust tests/builds or canonical status writes were performed.

Combined colour/thumbnail regressions now cover saved palette replacement during
an actual row fade, discarding current/previous old brushes while retaining the
decoded cache and real physical-membership roles. The colour-count render fixture
explicitly disables fade and blanks the extracted paint images. Nonempty-background
exit Cancel/accept/re-show/fresh-binding coverage is authored; hosted execution
remains pending. This correction adds no completion credit or renderer fidelity.

The combined colour/thumbnail integration copies all eight saved local/remote and selected/unselected fill/border roles into each owned paint snapshot. Current physical storage membership selects the palette, including collection membership. A colour change clears both old and current copied cells before repaint; the viewport background keeps its saved grid colour behind the clipped image. This integration remains source-only until exact hosted native execution and PNG inspection.

The bounded file-size comparison radio update preserves the existing typed
query executor and all other predicate widgets. Main-owned predicate children
now require a visible, current binding before opening and cannot accept across
rebind or accepted exit. Their existing page-identity and lock checks remain.
The recorded default Enter path is supported; the separate global radio-Enter
preference and generic active-predicate editing remain outside this slice.
Supplied-value reconstruction is covered in the model and saved-default
reopening in the native owner. The documented reference TB text defect and
free-text parser limit remain. Structural predicate/Options parents stay
Partial and uncounted. The three selected filesize/hash leaves passed full
Linux execution and independent rendered review at `170ab0525`.

The new filesize/hash owner retires on actual Main-component disappearance
through a repeated 50 ms weak-owner check, or immediately on final Bound-owner
drop, rebind or accepted exit. Temporary hide and pending/declined exit do not
retire it. Passing tests retain child/slot/page/headless adapters before checking
automatic closure and stale input refusal; the private notice component is not
separately exposed. The actual Qt `parent_destruction` recording demonstrates
standalone QWidget.window() parent cascades for shown nonmodal filesize/hash
DialogEdit/FleshOutPredicatePanel children, with five wrappers retained per case.
It preserves all ten original hash fixture keys and unchanged blank predicates;
client Main survives. Client Main destruction, nested exec teardown and active
nested-warning destruction are not recorded. Native nested-notice teardown is
a separate safety boundary; the failed Qt nested-warning attempt earns no claim.

The inspected `hash-predicate-cleanup-warning-native.png` captures the actual
recorded four-section cleanup message, while `hash-predicate-warning-native.png`
remains the invalid-acceptance notice. Native warnings omit Qt's warning icon and
platform chrome; forced cleanup keeps the inline "You sure?" question. Typed
hash sets reconstruct in deterministic sorted order where Qt can preserve tuple
order; raw cleanup preserves first valid occurrence order. The TB text/parser
limits above remain. Only filesize and both hash leaves are signed off; generic
active editing, radio preferences, predicate parents/aliases and deferred
Windows/macOS receive no credit. Full exact-source Linux validation and fresh
independent render review passed: 346 signed off, 29 candidates pending.
Evidence (checkpoint `170ab0525`, in git history before 2026-10-08) preserves the standalone
Qt lifetime scope and failed nested-warning experiment without claiming parity.

The two archive/delete finish policies now reach selectable deletion scopes and
an owned 1.2-second multiple-button guard. The native finish and Forget questions
are modal layers within the existing filter canvas; Qt creates separate dialogs.
Secondary local-domain choices follow deterministic native service order, while
Qt obtains those secondary choices from set iteration; the page/all-local priority,
labels, scope counts and selected transaction remain equivalent. Arbitrary broader
archive/delete viewer/shortcut topology is not claimed. Stale replaced service keys
reject the entire transaction instead of selecting a replacement domain. Actual
Qt controls and 48 finish paths were recorded; source checks and authored tests
are separate from pending hosted Rust and exact-source native PNG review.

## Owned debug long-text producer

The finite Help/debug/gui-actions “make a long text popup” action now reproduces
both reference JobStatus publications, its five random words, 124 scheduled
text/title updates and 200 ms cadence. It uses Store popup jobs and the ordinary
native toaster rather than a separate debug display. The native producer is
owned by one main binding: accepted exit, rebind and owner destruction cancel
its timer and pending strings; dismissed cards release their future updates at
the next live boundary. Qt schedules independent CallLater setters that may
outlive a dismissed popup. Existing hidden-window backend updates continue,
matching Qt, and hidden owners cannot launch new sequences. Slint uses the
existing native font-derived popup width policy; Qt pixel geometry is recorded
without claiming identical font metrics. Other debug actions and broader popup
freeze/monitor/API families remain Partial. No MIME-mode reassessment or parent
completion is claimed. Native regressions are authored; hosted execution is pending.

Physical deletion now consumes the saved files-and-trash per-pair delay rather
than deleting an entire native batch without waits. The real Qt control uses
seconds 0–59 and milliseconds 0–999, minimum 20 ms, default 600 ms; raw imported values
are retained until Apply. Constructor truncation and exactly one acceptance
conversion match 16 actual staged/Cancel/Apply/reopen cases, including 1029 ms
displaying 1 s 28 ms and explicitly entering 1 s 29 ms saving 1029 ms. The native-wins
retained ClientOptions upgrade avoids silently replacing older saved waits.

Both explicit CLI purge and the daemon's physical worker commit a pair then wait
outside the writer, including after the final pair or a missing physical file.
The pass captures its preference once. Real re-add/import claims and shared-media
guards are rechecked for every admitted pair. Shutdown retires the owned worker,
wakes bounded wait slices and prevents later admissions; a current filesystem
call cannot be interrupted. Errors abort the pass and retain its failed queue
entry without undoing earlier committed clears. Actual Qt ordinary-delete
refusal is recorded and has no wait/queue clear. Corrupt native metadata raises
an error; reference-specific boot-long disabling and diagnostic text remain
separate broader maintenance behavior. Existing native 1024-pair/ten-minute
passes and broader scheduling/location-extension repair remain separate;
the finite normal-time admission controls below now reach these workers. No credit is proposed for those worker/maintenance parents.
This finite delay-control proposal requires exact hosted Rust/native validation
and inspection of its authored Options artifact; local Cargo/Rust execution was
not performed. All physical mutation evidence uses disposable authored/copied
fixtures, with reference source read-only.

Already restored local media only clears a stale deferred queue; it performs no
physical deletion and consumes no physical-pair wait. The reference clears those
queues on local re-add before its deletion loop.

## Radio Return policy and default-button delegation

The finite GUI/misc checkbox now has a typed saved default, legacy decoding and
old-imported-store fallback, independent staged saving, and live key consumers
in every current native radio list: filesize comparison, hash sign/type and
advanced deletion action/reason. Qt's EnterCatchingRadioButton reads the setting
on every Return/Enter press. True ignores the key to promote it to the dialog;
false delegates to QRadioButton's platform behavior. In all forty recorded Linux
modal cases, both branches ignore the radio event and the actual parent default
Apply accepts, including changes after opening. Native true uses its explicit OK
callback; false rejects to the actual parent bubbling/default route. This preserves
the observed outcomes without inventing suppression when disabled. Advanced
deletion no longer captures Return before its radio children can read the policy;
Escape still captures, and the custom reason LineEdit still accepts normally.
Hash TextEdit consumes newline before the default route. Other Qt/platform widget
fallback behavior is not asserted, nor are unrelated dialog families promoted.
Actual reference recording and Store/model/native regressions are pinned. The
October 7 rerun preserves all 40 recorded cases exactly; the new saved-disabled
Options capture and retained unforced-default regression passed exact-source
Linux execution and fresh independent rendered review at `32d9dbb74`.
Evidence (checkpoint `32d9dbb74`, in git history before 2026-10-08) approves exactly this
preference leaf. The Qt consumer image is
re-shown after acceptance, while the native consumer image records pre-key pixels;
neither proves key handling through pixels alone. No matching Qt Options PNG is
recorded; its staged/reopened state is captured in JSON. Native lifetime refusal
with retained components is not Qt object-deallocation parity. Windows/macOS
remain deferred, and no parent, alias, predicate or deletion leaf gains credit.

## User namespace grouping Add/Edit

The tag-sort namespace grouping Add/Edit controls now store the reference's raw
ordered values, including blanks, colon sentinels, whitespace, case and duplicate
entries. Their Enter Text and removal questions use owned native SessionDialog
children; field-scoped TagPresentation saves preserve concurrent sort, display
and rendering preferences. The existing sort consumer already implements first
matching namespace precedence and the colon fallback, and remains unchanged.
The queue's clipboard Paste action is still absent, so the broader grouping and
Options parents remain Partial. This slice does not claim the already implemented
nested file-sort parser or its tag-display chooser, generic predicate editing,
radio-Enter policy, aliases, or any parent. Actual Qt recorded 18 queue paths and
six downstream sort orders plus three real modal Enter Text handlers and both Qt
PNGs; Rust/native assertions and fresh PNG captures passed full Linux execution
and independent rendered review at `7c74c6171`.

The validated shared layout follow-up includes an actual blank-valid Add frame and
a populated Edit frame. The reference Enter Text PNG contains blank input;
the retained native Add-default frame contains `namespace`, a different valid
state. No matching populated Qt Edit PNG or pixel equality is claimed. Raw
values, stable identities, cancellation and concurrent-field saving remain
unchanged; the broader grouping queue/Paste and Options parents remain excluded.

The popup question label continues to wrap in the native client. Actual Qt `PopupMessage._text_yes_no` remains a single-line label at the same narrow/fixed width settings; the new `popup_question_layout.json`/PNG records that distinction and verifies every Qt action control fits within its card. The native layout repair preserves its existing wrapping while preventing the lower stop button from crossing the clipped card boundary. No Options or popup family completion status changes.

The two Files and Trash normal-time controls now reach actual automatic trash
and deferred physical-delete passes. Defaults and retained ClientOptions imports
are true, native preferences win, and Options saves only changed fields. A GUI
owner samples its existing live activity/API idle monitor immediately before each
pass; a daemon without GUI activity is normal time. Idle bypasses either unchecked
normal-time flag, as Qt does. An admitted pass is not cancelled by a later flag
edit. Explicit CLI purge remains an explicit command rather than an automatic pass.

GUI maintenance runs off the UI thread, with at most one worker of each kind.
Rebind, accepted exit and last-owner drop cancel the old workers, wake physical
waits and reject late reports; a declined exit leaves them live. Trash checks
shutdown before each eight-file group, while physical deletion retains its per-pair
writer admission, current-storage/import/shared-media checks, durable queue clear
and outside-writer captured delay. A current filesystem call or admitted group
may finish. Errors stop a pass and are retained as owned diagnostics.

This is bounded gate parity, not the broader maintenance scheduler: existing
256-file trash writes and 1024-pair/ten-minute physical passes, queue wakeups,
reference inter-group trash pacing, CPU/system-busy/global mouse activity, boot
error disabling and location-extension repair remain Partial. Actual Qt entry,
mid-pass settings and shutdown paths were recorded with an owned synthetic queue
transport; physical native tests use disposable copied media. Authored Rust/native
regressions and one Options artifact await hosted execution and render inspection.
No local Cargo/Rust validation was run; no parent completion is proposed.

The delayed debug-popup action reuses the main binding’s owned producer instead of a process-wide background scheduler. Its clock is monotonic and each launch has a five-second deadline; event-loop delivery can occur later. A weak single-shot timer follows the nearest deadline, capped to the existing 200 ms liveness/dismissal poll, preserving long-popup pruning. Cancellation on GUI retirement is explicit; already-published completed jobs remain independently dismissible. The real Qt recorder uses the genuine scheduler and real elapsed observations, while native authored regressions use an owned deterministic clock; these are distinct evidence boundaries, with native runtime/rendering still hosted-only. Other Debug actions and import-favourites deletion’s recorded single-row reference exception remain outside this leaf.

The automatic maintenance binding has a private shared retirement owner: dropping the final Bound clone
permanently cancels its workers even if the MainWindow callbacks or a public
Control remain retained. A dropped MainWindow is detected by its weak-owner timer
or the next poll; callbacks never substitute a successor window. An authored
held-wait regression keeps the emitting main and Control alive, drops each Bound clone,
checks the remaining queue and lets only a fresh binding consume its next pair.

The delayed new-page debug action preserves Qt’s Help-menu-construction location
snapshot (rather than rereading it at trigger or delivery), current notebook at
delivery, and current default tag service. Actual Qt scheduler observations and
PNG are separate from native deterministic-clock regressions, which have not run
locally. Native binding retirement disposes pending work permanently. The
separate “refresh pages menu in five seconds” action remains absent and unclaimed:
the native Pages menu already regenerates Store/page facts on each opening, so
this slice does not invent a refresh callback or promote that action, its parent,
or broader diagnostics families.

## Live wheel policy for represented menu choices

The native GUI now reads the saved default-enabled MenuChoiceButton wheel
preference per physical event. It cycles one choice by vertical sign, wraps,
handles horizontal/zero-y events, and preserves single-choice signals and empty
choice consumption. The shared wrapper delegates pointer expansion and popup/key
selection to the existing Slint ComboBox. Its front wheel handler prevents that
widget's independent focused-wheel behavior from bypassing the policy: ignored
wheels temporarily disable the child during dispatch, then the containing handler
restores it synchronously and rejects to the actual parent scroll area. No timer,
process-global mutable preference, copied standard widget or new menu engine is
introduced. Store import/load uses typed-native precedence over retained legacy
ClientOptions, and staged Apply/Cancel preserves concurrent other settings.

Only actual represented Qt MenuChoiceButton counterparts are wrapped: media
type and order (including the actual Qt container wheel producer), Options tag-sort controls and
live ManageTags and manual-export tag sorting. Other ordinary ComboBoxes remain unchanged. Qt's absent
native duplicate-filter grouping/potential-duplicate sort, metadata-importer
tag-display and inline main/viewer TagSortControl surfaces remain outside this
finite control scope and their broader families stay Partial. The per-row Options
text/count-order memory correction is necessary reference behavior, with zero
additional completion credit. The one original wheel-preference leaf remains
Partial with zero completion credit; no GUI/options parent, generic ComboBox or platform-menu credit.

Actual Qt recording covers eleven handler cases, ten live TagSortControl transitions,
three media-order events, nine represented media-type events with all 33 flat
identities, staged defaults/Cancel/Apply/reopen and a saved false
legacy tuple. The offscreen QWindow transport did not automatically forward an
ignored child wheel to QScrollArea; the recorder explicitly delivers that ignored
event to the unchanged real parent viewport and discloses this adapter. It is not
an attestation of automatic Qt propagation. Authored native regressions use
physical wheel/pointer/key events, actual Options ScrollView bubbling, real query
and tag-row consumers, and owner retirement; their execution and exact-source
PNG inspection are pending hosted CI. Pointer popup-row probing is bounded to a
160px band covering the native two-row menu across widget styles and retains mandatory exact
selection/results assertions.

The next wheel-test follow-up replaces the small resize readiness workaround
with an idle interval at the original viewport. Slint 1.18.1's Flickable can
retain a wheel-routing timestamp for 800 ms even when content is clamped and
no animation is active. Pumping real timers/layout for at least 810 ms tests
one distinct subsequent gesture without retrying input or changing assertions.
This source-supported precondition does not prove the earlier failure's precise
cause or immediate hide/show recovery. Full Linux run `37618131717` passed
this finite after-expiry precondition; no new wheel completion is claimed.

The actual Qt media-type Random roundtrip is recorded separately as a remaining
limitation: SHA-256 hash sort (system20) ASC → Random (system4) → hash ASC
retains the hidden lexicographic/reverse lexicographic choices. Native controls
show Random order choices and infer labels from the current sort, so they cannot
restore that retained hidden order on return. The successful non-Random type
consumer paths do not establish full MediaSortControl parity. No completion
claim is made for the preference, absent sort surfaces, generic ComboBoxes or
parents. Final actual Qt run was 2026-10-05 14:05:36–38 UTC; the two Random
steps remain reference-only evidence rather than passing native parity tests.


The cookie/header Client API notification preference reaches the actual native
routes and ordinary finished Store jobs, without adding a second popup scheduler.
It preserves the recorded reference quirk: an altered-only header request shows
“Headers sent from API:” without altered details; altered lines appear only when
a newly set header is also present. Failed or no-op requests do not publish a
success notification; accepted preceding header changes survive a later missing
entry error. Existing network request validation remains unchanged.
The Qt recording invokes genuine access-key establishment, permissions and
resource handlers on a copied client, rather than an HTTP socket; native authored
regressions exercise the real authenticated Axum router and owned Slint toaster.
Natural five-second strict expiry is replayed at Store/API level; the native
refresh regression advances persisted deadlines to verify live removal without
waiting on wall time. Broader toaster freeze/monitor/position and network backend
families remain Partial. Hosted exact-source Rust/Clippy/native execution and
three authored PNG inspections are pending; no local Rust validation was run.

The three image-cache controls govern real decoded full-resolution raster reuse,
not the preview's separate 64 MiB per-page accepted-frame snapshots or a viewer's
current source/resize buffers. Accounting preserves Qt's pre-decode RGB estimate,
loaded-footprint adjustment at access, strict admission equality, one-item soft
overflow and strictly older last-access expiry. Raster references are shared
without copying their pixel vectors; presentation references survive eviction.
The cache is owned by one main GUI binding, not a process-global controller; filters
opened independently by auto-resolution own their own policy-bound cache. Existing
duplicate pair prefetch warms this cache and retains no separate future raster
store. The total prefetch percentage, controller-wide sharing, image tiles,
video buffers and complete Qt scheduling/rendering families remain Partial and
receive no additional credit. Native failed full decodes use an uncached poster
fallback instead of retaining Qt's synthetic error renderer. Injected public
preview decoders keep their existing owned test/backend contract independently
of normal decoded-cache admission. The screen-count caption is omitted when the
backend has no monitor information (including the headless adapter); the pixel
budget remains available. Runtime/native render validation remains hosted-only.


Force idle mode has a real current-binding menu/Monitor consumer, but remains
Partial. The reference publishes `wake_idle_workers` on both toggles and subscribes
database maintenance and repository synchronisation to that topic. Native owned
idle-aware DB/repository wake subscribers, CPU/system-busy monitoring and the
reference’s `idle_started = 0` very-idle classification are not implemented by this
slice. It does not emit a fabricated wake event or reset trash/deferred/autosave
deadlines: actual Qt trash does not subscribe to force-idle wake, and deferred
physical deletes subscribe to their separate new-delete notification. These
existing native jobs consume the changed classification at their scheduled entry;
an already admitted pass keeps its snapshot. Runtime override state belongs to
the main incarnation and is not imported/persisted; shutdown/retirement wins over
forced idle. The recorder runs the genuine QAction and controller/worker bodies
with private clock/force globals and synthetic worker database transport, leaving
reference source/media and shared clocks unchanged. No local Rust/Cargo validation
ran; hosted execution/rendering is pending and no completion is proposed.

## Popup toaster minimized freeze (Partial)

The one original minimized-toaster control is implemented for genuine available
window-state signals, with typed-native precedence over retained ClientOptions,
legacy import, staged Apply/Cancel/reopen and field-scoped saves. Its live UI gate
preserves cached rows, summary, construction-time widths and pending admission;
Store producers and strict dismissal deadlines continue while frozen. Hidden UI
freezing is a required consumer correction with zero additional credit. The
existing GUI-incarnation retirement and shared binding ownership remain; no new
background timer or process-global state is introduced. Legacy dismiss/pause/
cancel/show-files/traceback actions now share weak visible/minimized/active input
guards, and keyed job actions retain their exact GUI owner checks.

Windows/macOS/X11 Winit reports actual minimized state. Wayland returns None;
that unavailable signal is retained explicitly and does not fabricate focus,
occlusion or minimized status. Headless regressions drive the genuine Slint
software-window minimized property, not an OS event attestation. Other-display
freezing still needs a global cursor-monitor signal unavailable through the
current backend and remains Missing. The broad hide/freeze family is unclaimed.
Exactly one original concrete Missing→Partial improvement, zero completions or
parents, is proposed.

Actual Qt 2026-10-05 14:26:46–48 drives unchanged private PopupMessageManager
AddMessage/REPEATINGUpdate under genuine Qt show/hide/minimized states, including
an actually unfocused window that still updates. Constructor background timers
are captured for manual ticks; held integer time and an explicitly due private
job regular checker expose strict expiry without changing the global floating
scheduler clock. Two Qt PNGs were individually inspected. Native retains its
existing bulk expired-row reconciliation rather than Qt's one-card-per-tick
removal cadence. Three historical setup show() calls were added to the existing
popup replay and session-warning owners; every assertion/deadline is unchanged.
All new Rust/native regressions and the restored-toaster native PNG are authored
only, with hosted physical/platform/runtime/render verification pending.

The finite FFMPEG call-timeout preference reproduces the reference subprocess
runner’s three-second communicate/check slices: a stored/displayed integer n
supplies ceil(n/3)×3 seconds to bounded default-configured calls. A saved value of
1 therefore permits a two-second call and times out a four-second call near three
seconds. The immutable deadline is captured before each subprocess; explicitly
configured `Ffmpeg::timeout(Duration)` retains its original precise API behavior
and wins over the Store provider. Streaming frame reads keep their existing fixed
per-chunk timeout and Drop kill/wait ownership; no player reuse/stream policy is
claimed. About preserves its pre-existing unavailable-version UI on timeout,
rather than showing the reference’s diagnostic path string. This records a real
local executable transport, not a codec benchmark or network request. Other
FFMPEG executable-discovery controls and broader media playback remain Partial;
only the original call-timeout leaf is proposed conditionally on hosted validation.
The deadline reader is shared by all current Store-aware GUI/model FFmpeg
construction paths, including review/duplicate/parser/folder helpers. The Store
keeps a weak self handle solely to support existing borrowed APIs; its database,
snapshot, construction ordering and ownership stay unchanged, with no new pools
or strong self cycle. Non-Store helper APIs keep default tools, and explicitly
configured executable/fixed-deadline APIs remain available. The new decoder
regressions use disclosed POSIX FIFO executable transports, not assertions of
codec correctness; platform-specific execution awaits hosted Linux/macOS tests.

The selected thumbnail clear-deletion-record action uses the existing scoped
local-file writer, distinct from the global all-files service action. Qt captures
physical-deleted flat media before confirmation and writes 64-record packages;
the native model preserves those identities, order and independent commits. A
file newly deleted during confirmation stays outside that plan; a re-added file
keeps current membership. No filesystem deletion or queue cancellation is added.
The native main additionally refuses retained hidden/retired/binding-replaced or
different-current-page callbacks, including a tab departure/return, and
blocks its owned advanced-deletion child. Native failures use the existing main
question dialog with a no-op acknowledgment, keeping the error visible across
refresh without retrying the writer; Qt's modal question ordinarily prevents such
synthetic dispatch. Both recorded old/default thumbnail menus expose this action;
no unsupported viewer action or broader media-context parent is claimed. Actual
Qt recordings completed 2026-10-05 15:27:50–51. The copied fixture's sandbox-denied
Client API listener logged a nonfatal startup warning; these GUI/SQLite recordings
make no socket/API attestation. Main now supplies the reference's clickable
yes/no choices through its existing answer callback, retaining keyboard answers.
The new pointer and key regression covers the captured record set, complete
relevant table state, physical queue, hidden answer refusal and a held press
after processed Escape retirement before a usable successor answer. It does not
claim unobserved atomic replacement or generic question ownership parity. The
reference image shows the menu, not a matched confirmation frame. The new
controls and capture passed full Linux run `37624387284` and fresh independent
rendered review. Only the selected thumbnail clear-deletion-record leaf gains
credit. Evidence (checkpoint `1c2b1afaf`, in git history before 2026-10-08).

### Viewer image-prefetch controls (source proposal, runtime pending)

The three original concrete speed/memory controls—per-viewer cache percentage,
previous count and next count—now reach the actual media viewer, archive/delete
filter and duplicate filter through the owned decoded-raster cache. Actual Qt
recording contains 78 circular neighbour orders, five control save/reopen cases,
six real image-renderer readiness/budget traces, six finished-only atomic flush
boundaries, five filetype display-action cases, and shown/hidden Canvas calls.
The supporting Qt controls PNG was individually inspected. Equality is allowed
by the prefetch planner but excluded by strict cache admission; successful
uncached equality decodes can consequently repeat on readiness, as Qt does.
Ready accesses recount actual RGB/RGBA bytes, unknown-resolution misses stop the
ordered pass, and a blocked first candidate does not admit later small files.

This is the represented GeneralImage full-resolution raster path. Qt's global
renderer object graph, image-project codecs, GPU/tile caches and video renderer
prefetch remain broader Partial boundaries. The native per-owner warm worker
uses the existing disk decoder and shared Arc cache; current native rendering
remains synchronous and can perform an independent duplicate decode while warm
work is pending, rather than adopting Qt's shared asynchronous current renderer.
Decode failure keeps the native blank/fallback behavior, without introducing
Qt's error-placeholder renderer. Current presentation and accepted preview-page
snapshots are not prefetch-budget bytes and are never evicted by warming. No
prefetch family, tile/delay/duplicate-count control or parent credit is proposed.
The three control proposals are conditional on independent source review and
exact hosted Rust/native/physical/rendered validation; canonical coverage is
unchanged.

The debug fetch action has an owned native response window and system save
picker rather than the Qt modal questions. Its progress attachment remains the
shared native display-only download row: the reference's embedded stop/cog is
not claimed. The native popup's own stop reaches the actual Job cancellation,
while broader network-control/cog parity remains Partial. Native exception text
comes from the existing Rust HTTP engine. Each request uses the existing
GUI-local engine pattern rather than joining a separate daemon's running jobs;
this does not establish shared-daemon global scheduling parity. Only the original
“fetch a url” action is considered for conditional first-pass scope after review
and hosted validation, with no parent/network-control-family promotion.

Headless cleanup now requires an explicitly retained `headless::Windows` guard,
marked `must_use`, until the UI scope ends. The former thread-local fallback hid
components during TLS destruction and could release the last Store, whose writer
Drop joins another thread. Windows holds the loader lock during TLS destructors,
so that synchronization is prone to deadlock (Rust LocalKey platform-specific
behavior). The fallback is removed on every platform; all 39 discarded initializer
call sites at Third3f25 now retain a local guard. The other initializer/helper
scopes were inspected: no window factory keeps a collector only until returning
a window. Normal collector-clone ownership and Store durability are unchanged.
The former discarded-collector lifetime regression now explicitly drops its
collector before thread return, retains every Store/worker assertion and deadline,
and proves the visible component is already released. A Windows-only five-minute
CI step runs the three lifetime tests with uncaptured progress before the full
default-parallel suite. The obsolete Windows run's last completed test does not
identify its blocked test, and this source risk is not proof of that run's exact
cause. No local Rust execution, mutation run or completion credit is claimed.


The scheduled file-maintenance review implements its current-work tab using the
existing 27 native runners. New-work search, quick selection and scheduling are
still missing. The native review uses an inline owned yes/no confirmation; Qt
uses its modal question dialog. Gauges advance immediately before each physical
job, while committed file results drive thumbnail facts and redownload delivery.
Same-owner Clear/Refresh commands are drained between fetched batches before new
work is read, matching the reference's batch lock boundary; the exclusive native
physical lease stays held so this metadata-only Clear does not race file work.
Ordinary daemon/CLI passes defer immediately on contention rather than waiting
uncancellably. GUI force waits remain cancellable. No UI disk work or joins are
introduced, and each redownload URL is attempted independently with its own error.

This exact original Missing leaf remains Partial with zero completion credit:
reference count Refresh can publish while a physical batch is still running,
but the native actor services it at the next batch boundary. An independent
process's physical pass still delays native Clear until its pass lease is free;
Qt's manager releases its local lock between batches. Native publishes the
cancellable waiting job before physical admission; Qt first publishes after
admitting its batch. Broader backend notification/error presentation and
scheduler policy remain unclaimed. Rust/native execution and exact authored PNG
inspection are pending; no canonical ledger, parent or scheduling/search claim.

The debug current-session reload uses a unique private immutable snapshot row,
not the reference's fixed user-visible temporary named session; concurrent reloads
cannot overwrite one another's captured tree. Saving/loading runs off the UI
thread and temporary rows are removed even when delivery is retired. An in-flight
Store transaction completes and cleans its slot after retirement, but its reply
cannot change pages. Snapshot capture and final reconstruction use the existing
synchronous Store boundary on the UI thread; broader asynchronous session/large
library optimizations remain outside this one debug action. Qt's observed first
page/first-child and empty-thumbnail-selection reset is retained. The neighboring
manual save-last-session action and wider debug GUI/style families remain unclaimed.

## Database maintenance entries

- The native store keeps fewer derived caches (ADR-6), so each maintenance job
  regenerates what serves its purpose: the tag storage/display counts (one
  rebuild serves "all", "just pending" and "missing file repopulation"), the
  subtag search indexes (global, whichever tag service is chosen) and the
  in-memory sibling/parent graphs. Analyze is SQLite's `ANALYZE` (full) or
  `PRAGMA optimize` (soft). Orphan tables are per-tag-service tables whose
  service is gone.
- Local hashes cache, local tags cache, service info numbers, total pending
  count, similar files search tree, repopulate truncated mappings tables,
  resync combined deleted files and clear orphan hashed serialisables ask
  the reference's questions (and which service, where it asks) but find
  nothing to do: the native store has no such caches or separate tables
  (pending counts and service numbers are counted live; the similar-files
  index is built per search). They show the popups the reference shows on a
  client with nothing wrong ("Done with no errors found!", "Done! Rows
  recovered: 0", "No orphans found!").
- Get tables using definitions works on the native schema; the reference
  v688's read raises `NotImplementedError` from one of its modules, so it
  shows an error there. Its lines have no schema names (one database file).
- Recovering orphan file records doesn't open a page of the recovered files.
  Resyncing tag counts reports "N desynced tag counts in SERVICE!" per tag
  service, where the reference reports surplus or missing files per
  file/tag-service cache. Fixed invalid tags aren't written to a log.
- The jobs show their result popups but no step-by-step progress text, and
  can't be cancelled once started.

These Database menu entries have no native counterpart, because the native
store keeps none of the reference caches or tables they rebuild (ADR-1 in
`ARCHITECTURE.md`): regenerate > total pending count, service info numbers,
local hashes cache, local tags cache, similar files search tree, the three tag
display mappings cache entries, tag siblings and tag parents lookup caches;
check and repair > repopulate truncated mappings tables and resync combined
deleted files; clear orphan hashed serialisables; review deferred delete table
data; and the two "work deferred delete jobs" switches. Each asks the
reference's question and reports a clean run (the switches are saved but
change nothing). They are marked out of scope in `docs/rust/tracking/`.

## How boned am I?

- The search panel is a domain list and a typed tag/system predicate box (as in
  file history), not the reference's full read autocomplete with its results
  list; "all files ever imported or deleted" isn't offered as a domain.
- The files table is laid out as text columns rather than Qt's grid.

## File maintenance > add new work

- The search is a typed tag/system predicate box (as in file history), not
  the reference's read autocomplete, and always searches the default local
  file domain; there is no domain or tag-service button.
- The description and "Jobs added!" show in the tab rather than as message
  boxes.

## Review current sibling/parent sync

- hydrus-rs applies siblings and parents as it writes, so there is never
  work to show: every service reads as synced and "work hard now!" never
  appears. Repository "waiting on" lines can't arise without repositories.
- Retired and hidden review callbacks cannot change service memory or refresh;
  an old Close/X cannot dismiss its successor. This ownership repair does not
  add the reference's background sync work or progress controls.

## Auto-resolution rule export and import

- Rules export as hydrus-rs JSON, not the reference's serialised form, so
  the reference can't import them; hydrus-rs imports both. Comparator lists
  don't have their own export/import buttons, and an image on the clipboard
  isn't read (use "from png files").

## Options kept but not used

- These Options rows are kept and edited, as the reference keeps them, but
  nothing in hydrus-rs reads them yet: the preview window's own volume, the
  REQUESTS_CA_BUNDLE switch (hydrus-rs uses its own TLS roots), drag-and-drop
  export (no files can be dragged out yet), the Qt-only gui misc and frame
  switches, the hide-page signal, the URL drop page switch, mpv's null audio,
  legacy mediator, player reuse and setGeometry switches, every QtMediaPlayer
  row, system FFMPEG, truncated images and PIL (hydrus-rs decodes images its
  own way), the pinned duplicates hover, the preview window hovers, the
  other-display popup freeze, the image tile cache and video buffer (hydrus-rs
  renders whole images and leaves video to mpv), the file system wake wait,
  the system tray page (there is no tray icon) and the petition reason count.
- The mpv box lacks "Set a new mpv.conf on dialog ok?" and the audio device
  fetch button; the QtMediaPlayer box lacks its device choice and fetch button.
  The style page isn't offered: Slint has no Qt styles or stylesheets.

## Review vacuum data

- hydrus-rs has one database file (listed as "main"), not the reference's
  four. It vacuums in place with SQLite's `VACUUM` while every store
  connection is paused, rather than vacuuming into a copy and swapping it
  in with the connections closed; the effect on the file is the same.
- The window opens directly, without the "loading database data" popup.

## Idle-time maintenance

- The GUI publishes its idle state to a marker file the daemon reads; without
  a GUI (or once it stops publishing for 15 seconds) the daemon works in normal
  time. Idle time doesn't wait for the reference's two-minute boot delay beyond
  what the GUI's own idle check already applies.
- CPU use is read from Linux's `/proc/stat`; elsewhere "CPU busy" is never
  shown. As in the reference v688, CPU busyness only reaches the status bar.
- Similar-files search packets search 16 files at a time until the packet time
  passes, so a packet can run over by one batch.
- The status bar has no "hydrus busy" or database activity fields yet.

## Thumbnail manage > maintenance

- "Do it now" queues the job for the selected files and restricts immediate
  due work to that captured selection. If another physical-maintenance worker
  holds the lease, the queued work is deferred. There is no popup of its
  progress, and the focused file isn't cleared from the preview first.

## Shutdown maintenance

- The shutdown work is analyzing tables without statistics; there is no
  repository processing (remote repositories are out of scope). It runs on the
  UI thread before the window closes, without the reference's exit splash or
  its cancel button.
- Restart is offered on every platform (the reference hides it for frozen
  Linux builds).

## Content undo

- Only archive/inbox (thumbnail, viewer and filter actions through
  `media_actions`) and Manage Tags' applied changes are recorded; tag changes
  made elsewhere (write-tag menus, filename tagging, migration) are not yet.
  A content package listing several actions or services names them sorted,
  where the reference's set order varies.

## Per-filetype media handling

- "add" chooses the filetype from a column of buttons rather than the
  reference's filterable list. The editor offers mpv whatever the client's mpv
  availability, and shows no mpv/QtMediaPlayer advice beyond the reference's
  intro text.

## Duplicates page filtering

- The pair searches are typed predicates (as in file history), not read
  autocompletes, and keep the page's file domain; there is no count pause,
  estimate or optimisation cog: counts are exact and run once per change.
- Setting the shown files' relationship applies each pair once with the default
  merge options; the reference runs its merges twice so content propagates
  between all files.

## Help > debug actions

- "make some popups" leaves out the reference's popups whose buttons call
  back into the client (user call test, auto-account creation, gap
  downloader) and its network-job popup, and the test job's subjob doesn't
  start pulsing after two seconds.
- The "modal" popups are ordinary popups: hydrus-rs has no modal popup
  dialog.
- "reset multi-column list settings to default" asks, then has nothing to
  reset: hydrus-rs doesn't save list column widths.
- "force database commit" checkpoints SQLite's write-ahead log (hydrus-rs
  commits each write as it happens); "flush log" writes its line to
  standard error.

## Thumbnail manage > file relationships

- "set a relationship with custom metadata merge options" and "set selected
  collections as groups of alternates" aren't offered yet. The viewer's
  menu doesn't have the submenu yet.
- A group's "best quality file" is offered when the group has other files in
  the page's domain; the reference counts its members per domain the same
  way, but hydrus-rs doesn't distinguish a king outside the domain.

## Database > backup

- A backup holds hydrus-rs's one database file and the media directory
  (`client_files` there), not the reference's four .db files. The backup
  works with the database live (SQLite's online backup) rather than closing
  it, and shows its progress in an ordinary popup, not a modal one.
- "Simple" means the media is in one location, which may be outside the
  database directory (an imported client's); the reference requires its
  default `client_files`.

- Restoring refuses the store's own directory, including aliases. Startup takes
  the GUI lock before restoring or opening the database, and restore refuses an
  active serving process. It stages the database copy before changing the old
  database or SQLite sidecars, and removes the restart request only after a
  successful restore. Failed source copies preserve the old database and media.
  Media mirroring and the final database/sidecar replacement are not one atomic
  transaction: a later mirror/install failure or process crash can require
  manual recovery. A retained request can be retried after fixing its source or
  stopping the serving process, or cancelled by removing
  `restore_from_backup.txt` while the client is closed.

## Database > locations

- "manage granularity" runs its progress in an ordinary popup rather than
  a modal dialog, and its questions in button windows.
- The prefix folder moved first is the first in order, not a random one.
- The list selects one location; it sorts by location only.
- The rebalance runs from a non-modal popup and the window stays open
  (the reference closes the panel and shows a modal progress dialog).


## Validation pass: backup and migration failures

Cancelled backups report an incomplete operation and do not advance the last
successful backup time. Cancellation is checked between media files; the current
SQLite backup or file copy finishes before cancellation can be observed.
Unlike the reference's unconditional completion text, cancellation is not presented
as a successful backup.

Client granularity migration records the destination chosen by the physical mover,
including prefixes merged from different storage locations. An in-memory move
journal restores exact original paths on cancellation, move failure or database
publication failure; existing destination files are never overwritten. A process
crash or failure during rollback still requires manual recovery from a backup.

Failed duplicate auto-resolution approvals/denials reload actual pending pairs
and display the error, preserving unprocessed pairs after a partially committed
batch. The delayed popup remains a bounded implementation: it is checked between
chunks and cannot appear while the first long-running chunk is in progress.

## Validation pass: pending dialog ownership

Database maintenance questions and service choices now belong to their Main
binding. No, Cancel, native close, hidden input, rebind, accepted client exit and
final Bound release retire pending admission; retained callbacks cannot write or
displace another dialog. An already admitted worker may finish after its window
closes. Vacuum additionally requires a currently pending confirmation and eligible
selection, and accepts it only once. These repairs do not establish complete
reference parity for every maintenance command.

The shutdown maintenance question treats Cancel/native close as abandoning exit;
No and timed auto-no register skipped work and continue. Password dialogs accept
text only at a text step and Yes only at the current clear confirmation. Retired
password dialogs cannot change the lock. Network-error close callbacks hide only
their own window, preserving a replacement error.

Validation repairs now invalidate cached thumbnail colours when the stylesheet
changes and hide an exact retired colour picker on repeated Cancel. Wheel
controls publish their initial measured geometry as well as later changes.
These repairs passed full Linux replay at `56b93ae49` (696 GUI tests).
Completion credit remains limited to the 94 independently reviewed original
leaves banked across seven checkpoints, with the latest
334-item checkpoint (checkpoint `63f9e35ab`, in git history before 2026-10-08);
other repaired behavior retains its scoped assessment.

Speed and Memory helper overlap and favourites capture setup are repaired and
validated at `7c3c1aac5`; the preceding failed run is retained in the checkpoint.
All 697 GUI tests pass with existing assertions intact. The 18 new approvals are
finite: watcher timing controls are below the saved Options viewport, some
consumer captures lack matching Qt UI images, and neither headless images nor
injected clocks establish physical-display or wall-clock parity. The separate
thumbnail Debug clear-action evidence gap, colour-picker clipping, thumbnail
preview-checkbox input/enabled behavior and idle Options clipping remain pending.
Six earlier sidebar/tab descriptions are corrected without changing their
counts or assessments; original archived reviews remain unchanged.

Colour-picker geometry and thumbnail preview-checkbox behavior are validated
at `9bec37964`. The layout retains the native RGB-only picker
scope; it does not add Qt HSV/HTML/history features. Actual checkbox/dropdown input, displayed-state checks and
disabled-state observations preserve the existing staging, cancellation and
consumer assertions. Full Linux validation and fresh independent render review
approve these seven scoped leaves, bringing the ledger to 326 with 49 candidates
pending. The preceding paragraph records the earlier checkpoint; picker and
checkbox blockers are now resolved. Idle clipping and clear-action evidence
remain in the next bounded batch.

The idle layout repair preserves complete ignore captions and their existing
None/enabled interlocks. Its two-line presentation differs from Qt's horizontal
Noneable control. Only browsing/API threshold candidates are under validation;
mouse movement is still observed in application windows rather than globally.
Full Linux replay and fresh independent review at `c768fef48` approve only the
two selected thresholds. Constructor-state observations do not claim physical
checkbox input, and the unrelated CPU ignore caption remains clipped.

The preview and thumbnail-clear evidence additions do not change product
behavior. Preview captures show freshly bound default/saved Options states;
controlled interval timing, cap-before-minimum and cancellation checks remain.
The clear-menu and reloaded-thumbnail captures supplement cache-state assertions;
images alone do not establish access-history reset, generation or memory parity.
Qt/native storage accounting and preview-acceptance differences remain scoped.

Full Linux execution and fresh independent review at `c768fef48` approve the
two preview controls and clear-thumbnail-cache action alongside the two idle
thresholds: 331 signed off, 91 new across six October 6 checkpoints, 44 pending.
The earlier compile-only E0373 failure is retained; the repaired run passes all
698 GUI and 67 media tests. No matching Qt Options/menu images or full-pixel
parity are claimed. Prior checkpoint paragraphs remain historical records.

The duplicate-colour repair uses Qt Canvas's default black status text
instead of pale grey on the default white background. Saved role 11 still
overrides that fallback; it does not introduce automatic contrast selection or
arbitrary QSS parity. Existing A/B colour calculations, transparency samples,
staging and ownership assertions remain intact. The three existing colour
candidates passed full Linux execution and fresh independent review at
`63f9e35ab`: 334 signed off, 94 new across seven October 6 checkpoints, 41
pending. The two supplemental override images contain live opaque media and
later comparison results; they prove status colour, while the eight controlled
probe images retain background/clipping evidence. No broad pixel or palette
parity is claimed.

The presentation/appearance evidence batch changes test captures only.
Native tab/sidebar geometry remains distinct from Qt; hidden sidebar and ClearAll
states have source/JSON evidence rather than matching Qt screenshots. ClearAll
still advises restarting the client to see changed counters. Blurhash captures
use the real missing-source worker and finish the owned paint clock with saved
fade enabled; they do not establish whole-cell Qt blending parity. Background
captures compare the recorded 31x17 marker's extent/anchor and clear policy, not
whole-window pixels. Fade, renderer architecture and broader structural parents
remain outside the five selected leaves. Full Linux validation and fresh independent
review at `05000f10e` approve exactly these five scoped leaves: 339 signed off,
99 new across eight October 6 checkpoints, 36 pending. The 19 inspected captures
and 699 passing GUI tests preserve these limitations; no broader renderer/fade,
parent, alias or deferred-platform completion is claimed.

The validated suggested-tags width/layout captures grant no recent, related, lookup
or parent credit. Opening preferences remain owner-local; persisted most-used
broadcasts retain the existing 200ms observer. Native frames differ from the Qt
post-activation frame, its inline editing and Clear action. Current fixture CJK
glyph rendering differs while Unicode row/draft data remains asserted. The saved
width property is not a direct measured-geometry observation. Fresh review at
`e886e68ed` verifies readable list boundaries and layout; CJK glyph rendering
remains different from Qt. Exactly these two leaves are published, bringing
the total to 341 signed off with 34 existing candidates pending.

The validated namespace Add/Delete repair distinguishes the real entry widget's
empty-text veto from the existing handler-level fixture, which substitutes
Quick.EnterText. The new reference recorder drives actual Apply/Return and
warning acknowledgement. Its parent panel is shown standalone, mixed Delete
selection uses the list's real selection helper, and random RGB values are not
fixed expected colours. Native title metadata is asserted separately because
headless snapshots omit native title-bar decoration. No Namespace Edit, parent,
alias or deferred-platform completion is claimed. Full Linux validation and
independent review at `4b5e7ae15` approve exactly Add/Delete: 343 signed off,
32 existing candidates pending. Native dialogs differ in footer proportions,
font metrics and warning icons; automatic OS focus/modality and destruction
closure for every shown child are not established. Continued typing explicitly
clicks the retained LineEdit after acknowledgement.
