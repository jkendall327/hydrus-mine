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
- **The trash is emptied whether or not you are busy.** Hydrus skips its
  hourly trash maintenance while you are using the client, unless
  "maintain the trash in normal time" is on (it is by default). `hydrus
  serve` has no user to be busy, so it always runs it, with your maximum
  trash age and size; the switch isn't carried over.

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

- **The "edit favourite search" dialog's search is typed**, not the
  reference's whole autocomplete: a predicate typed and entered is added
  (as the page's search box adds what is typed), a double-click removes
  one, and the domains it searches are shown but not changed there; its
  sort and collect show as text beside "save sort" and "save collect", not
  as the page's controls. To change those, load the search into a page,
  change it there, and save it again. Its questions ("Remove all
  selected?", "already exists! Do you want to overwrite it?") are asked in
  the dialog rather than in a window of their own, and the list sorts
  names casefolded as lowercase (Python's casefold differs only for a few
  letters, such as "ß", which we fold to "ss" as it does).

- **System predicate editors type dates** ("2011-06-04", and "13:05") where
  the reference's have a calendar and a time box, and a viewing time is
  kept to the second (the reference keeps its milliseconds, though it
  never shows them; one stored by hydrus comes across to the nearest
  second). A file size in terabytes, which the reference's editor offers
  but can't write out ("error:cannot render this predicate"), is written
  "200TB"; neither parser takes "TB". Their radio buttons are drop-downs
  (as are the like/dislike and star controls of "system:rating"), and an
  editor's rows don't wrap: a narrow window scrolls sideways. A filetype
  group partly ticked shows unticked (Qt's tree shows it part-ticked).
  "system:hash"'s forced clean-up doesn't ask "You sure?" first, and what
  the reference warns of in a dialog is said under the panels.
  "Paste image!" takes a file's path from the clipboard, not image data.
  A recent predicate is forgotten with a "forget" button where the
  reference has a trash icon.

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
  sessions > "append backup" (hydrus-rs keeps no session backups, so
  saving over one keeps no backup of it either); and the
  undo menu's undo, redo and search history, which hydrus-rs doesn't keep.
  Hydrus's menu entries describe themselves in the status bar as the
  pointer passes; ours don't yet, and the history's latest page isn't in
  bold. Saving a session asks its name and its questions in one dialog,
  and says a name can't be had over the name box, where the reference
  shows a message box first; and "clear and load" isn't there yet.
- **The options window has only the options hydrus-rs honours** (so far
  those on twenty pages; the others, and pages with none, aren't there:
  on the connection page, the CA bundle and curl_cffi test; on the
  downloading page, the default download source, the
  number of subscriptions syncing at once and the failed-imports limit; on
  the maintenance page, idle time and shutdown, repository, sibling,
  deferred delete and idle work settings; on the duplicates page, the
  preparation tab's notification and the filter's colours; on the file
  viewing statistics page, the filters' own switches and the menus'
  stats; most of the gui page; on the importing page, dropped URLs and
  the work slots; and on the media playback page, the preview's zoom,
  re-centring, the checkerboard, animations, mpv, Qt's player and the
  system settings; on the file sort/collect page, the namespace sorts'
  list and the default collect's tag service; on the tag sort page, the
  manage tags dialogs' sorts (ours sort as the media viewer's list) and
  the namespace grouping list; on the ratings page, the example
  rating service's dropdown, the clickable examples, and the preview
  window's and dialogs' sizes; and on the thumbnails page, fading, the blurhash fallback, focusing on ctrl- and shift-selection,
  key navigation's scrolling, the scroll rate, the background image and
  the rendering tech). It opens on its first page,
  rather than "gui" or the page last open; options' tooltips aren't shown;
  a box's title is a heading over its options rather than a frame around
  them; a sort's type is a dropdown of the types a page's sort control
  lists, where the reference's is a button opening a menu of them, and a
  collect's choices are checkboxes under its label, with its unmatched
  files' choice, where the reference's are a dropdown and a cog menu; and a time behind a button in the reference (the downloaders'
  waits after errors) shows its fields in place. Its search suggests only
  the options it has, and their boxes (the reference's also suggests other
  text on its pages, such as units and dropdowns' choices), and is always
  at the top (the reference's "Put the options search bar at the" isn't
  an option yet); two options with the same label each go to their own
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
- **The "manage notes" dialog** copies every note as JSON, the
  reference's default; its cog menu's choices (copy just the note in
  view, copy as plain text, where the text cursor starts) aren't there.
  Its notices ("Copied 2 encoded notes!") show beside the buttons rather
  than as a passing note over them, and the reference's shortcut to
  apply it isn't bound. Double-clicking beside the tabs doesn't add a
  note.

- **Dragging across a list's rows with ctrl held** selects the range
  from the row pressed, as a plain drag does; the reference's adds the
  range to what was selected (or takes it away).

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
- **The "review actions" window** lists a rule's pending pairs by their
  groups; the reference lists them in its table's order (when they were
  queued), which hydrus-rs doesn't keep. Double-clicking a pair (the reference opens the
  duplicate filter, approving or denying, on the pending pairs, or the
  media viewer on a pair) and the lists' right-click "show in a new page"
  aren't there yet. Approving and denying happen at once, without the
  reference's "approving: 1/4" progress and popup.
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

## Local imports (`hydrus-download::queue`)

Checked by `crates/hydrus-download/tests/local_import.rs`.

- **A local import (the reference's "import" page, `HDDImport`) is an
  import queue the daemon works**, as it does a URL downloader page's: its
  files are imported in order from their paths, each with its modified
  time as its source time, a missing one vetoed ("Source file does not
  exist!"), and, if the import says, each one in the database afterwards
  deleted (to the recycle bin, if the options say). Its sidecars (the
  reference's metadata routers) aren't supported yet; the tags to add to each file (from an "import"
  page carried over from hydrus) are added as a downloader's are.

Checked by `crates/hydrus-gui/tests/gui/local_import_dialog.rs` (against
`oracle/fixtures/local_import_dialog.json`) and
`crates/hydrus-gui/tests/gui/import_files.rs`.

- **The "review files to import" window parses its paths as the
  reference's does** (the same rows, order, filetypes, sizes, progress
  text and files to import, folders with and without their subfolders,
  sidecars and `Thumbs.db` set aside), but **it has no file or folder
  picker**: paths are typed or pasted into a box over its list, or dropped
  on it or the main window. Its "add tags/urls with the import >>" button
  opens the "filename tagging" dialog, which has no "sidecars" tab yet;
  its tags are typed a line each (the reference has a tags input with
  autocomplete and paste buttons), as are its quick namespaces
  ("namespace:regex") and regexes, where the reference has lists with
  add and edit dialogs.
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
- **The manage subscriptions dialog is a first pass.** It lists the
  subscriptions and can delete, pause/resume, scrub delays, check
  queries now and select by query text, add and edit subscriptions,
  merge, separate, lowercase, retry, reset, and overwrite downloader and
  checker options, and deduplicate. It has no "export"/"import"
  or import options (copy, paste, clear) buttons yet. "merge" merges each group
  as its questions are answered (cancelling a later group's questions
  leaves the earlier merged, where the reference merges none). It doesn't reckon bandwidth waits (the
  error/delay column is empty unless the subscription is delayed). It
  doesn't pause subscriptions while open, as the reference does: "apply"
  writes only what the dialog changed, so a subscription the daemon ran
  meanwhile keeps what the run found, unless the dialog changed the same
  query.
- **The edit subscription dialog** has no multi-site downloader warning, and
  no "additional tags" or file log compaction number in the query editor.
  Its downloader choice is one list (the reference puts the downloaders
  not on show, and those that don't work, under further entries); its
  retry buttons are two buttons where the reference has a menu. Editing a
  query's text to differ only in case is allowed (the reference refuses
  it, as a clash with itself), and renaming a subscription to differ only
  in case doesn't add " (1)" (the reference counts its old name as
  taken). Its queries' bandwidth waits ("recent delays") aren't reckoned.
- **The import and export folders dialogs** don't edit sidecars yet
  (they show what is set, which is kept). An import folder's filename
  tagging is added for a tag service chosen from a list beside "add" (the
  reference asks which in a dialog), and edited in the "filename tagging"
  dialog's boxes (see "review files to import"). An export folder's
  query is typed as the Client API's tags rather than through the search
  autocomplete, and its sidecars can't be tested on example files. They
  don't pause the folders while open, as the reference does.
- **The duplicates page's preparation tab** has no "regenerate search
  tree" or "regenerate search numbers" (hydrus-rs builds its search index
  afresh, and counts searched files directly) or "resync potential pairs
  to storage" yet. Working hard tells `hydrus serve` to search whatever
  its idle and normal time switches say (it has no idle time of its own).
  The tab's name always hides the percentage once over 99% done (the
  reference's option for it isn't kept). The auto-resolution tab can't
  review rules' actions yet, and a rule's status can't say
  which rule `hydrus serve` is working on ("searching", "resolving"): it
  says "working".
- **The auto-resolution rule editor** takes its searches as terms typed
  a line each (as the Client API reads them), in the rule's location
  (which isn't changed there), where the reference has search
  autocompletes with a location button and a live count of pairs. It has
  no "preview" tab, nor import/export/duplicate of rules or comparators,
  and custom merge options start from the client's for the action and
  aren't edited there. A relative comparator's time delta and range are
  in milliseconds, where the reference has a time widget.
- **The file log window** can't yet import new sources, export them to
  a png, search for the selected URLs, or do its advanced entries (these
  are greyed out); its "additional urls" don't show the URL a URL class
  would actually fetch or refer from; trying a previously deleted file
  again doesn't offer to clear its deletion record.
- **The import options editor** doesn't edit a tag service's "get tags"
  filter (shown as what it lets through),
  and takes the tag filtering blacklist, the whitelist, additional tags
  and note names as typed lines (the reference has a tag filter editor,
  a tags input with autocomplete, and list editors; its note renames
  are a two-column list, typed here as "parser name -> saved name").
  The tags page's "set a filter for already-exist test" isn't there.
  Locations take one destination (the reference's takes several), and
  presentation's location is all my files or all local files. It has no
  copy, paste or favourites buttons, and always lists kinds as the
  reference's "simple mode" does (hydrus-rs has no option for it yet).
- **"clear and load" a session**: when pages object to closing, the
  question has "yes" and "no" (the reference's also has "no, but show me
  the pages", and its "yes" is only enabled after a moment).
- **The search log window** can't yet export URLs to a png, import new
  URLs, or export the selected page objects (greyed out).
- **A subscription query's logs**, opened from the query editor, change
  the query at once; the reference edits a copy that the dialogs'
  "apply" keeps or "cancel" drops. A query added in the dialog has no
  logs to open until it is applied. Likewise an import folder's file log.
- **Subscriptions run one at a time**, as with the reference's default
  `max_simultaneous_subscriptions`; a higher setting comes across but
  isn't used yet.
- **Import options that run a program on each imported file are kept but
  not run yet.** The migration warns where they are set (which defaults,
  subscriptions, import folders or downloader pages).
- **There is no browser impersonation** (the reference's optional
  `curl_cffi` connections); requests are always plain ones.

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
- **A popup's `network_job`** says its URL, status, speed, bytes read and to
  read, whether it is done and whether it failed, as the reference's does;
  hydrus-rs's network jobs don't say whether they are waiting on a
  connection error, the domain, the server's bandwidth or the engine, so
  those read `false`, `true`, `false` and `false` (as for a job that
  isn't), and `total_data_used` is what this request has read.
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
