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
  `crates/hydrus-gui/tests/session.rs` cover the rest.
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

- **Thumbnails on a scaled screen are resampled to its pixels** (area
  when shrinking, Lanczos when growing, as the reference resizes
  thumbnails), where the reference has Qt scale them as it draws. Slint's
  software renderer scales images by picking the nearest pixels, which
  made them blocky, so we never let it scale one. The thumbnail border
  and margin are hydrus's defaults (1 and 2 pixels); yours aren't carried
  over yet.

- **The page chooser takes the top row's digits too.** The reference takes
  only the number pad's; Slint doesn't tell them apart.
- **A closed URL downloader page's downloads wait** until it is reopened
  (Ctrl+U), and are deleted with it after the hour or when the client
  closes (or, after a crash, when it next opens). The reference's closed
  page imports on out of sight until it is destroyed; ours stops, as the
  close question's "This page is still importing." suggests, and a daemon
  left running without the client doesn't work on a page nobody can see.
  `crates/hydrus-cli/tests/serve.rs` and `crates/hydrus-gui/tests/session.rs`
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
- **A gallery or watcher downloader page's list selects one search or
  watcher at a time**, and its buttons act on that one; the reference's
  lists select several. A watcher page has no checker or import options
  buttons yet (the page's options, from hydrus, are given to new
  watchers), nor "update selected with current options". Its
  downloader list is flat (the reference nests a site's downloaders and
  greys out ones that can't work), and its searches' import options and
  file limits can't be edited from the page yet. Its columns are as
  narrow as the reference's, which shows the status columns as single
  characters; with Slint's table they take what room the sidebar has.

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
- **No popups or pages.** Without a GUI, "show a popup while working" and
  "publish files to a page or popup button" do nothing; what a check found
  and imported is logged.
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
- **No popups.** What a run exported and removed is logged.

- **Where the filesystem ignores case, a moved sidecar keeps the
  lower-case spelling** (`a.png.txt`): both spellings seem to exist there,
  and we take them in the order sidecars are read. The reference, written
  for Linux, takes them in its set's order, so either may win.

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
- **Subscription messages go to the log** (and the subscription runner's
  status) rather than popups, until there is a GUI; so do new files a
  subscription would publish to a popup button or page.
- **Subscription changes made from the command line reach a running
  `hydrus serve` within five minutes.**
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

- **Every popup is in view**, and a dismissed one leaves the list at once
  (the reference's GUI shows a limited number, and clears dismissed popups
  on its next refresh, within a second or so).
- **Only popups made through the API are listed.** The reference also lists
  its own jobs (downloads, maintenance, errors) as popups; hydrus-rs logs
  those instead, so far.

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
