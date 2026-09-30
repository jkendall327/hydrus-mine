# Downloader design

Status: parsing, the HTTP engine, import options, URL queues and
subscriptions work: `/add_urls/add_url` (Hydrus Companion's "send to
hydrus") downloads posts and files into named queues, and subscriptions
(migrated with their history) are checked on their schedule while `hydrus
serve` runs. `hydrus subscriptions <store> ...` lists subscriptions and adds
many queries at once. Thread URLs sent to `/add_urls/add_url` start
watchers, which check the thread on the reference's timing until it 404s
or goes quiet. Gallery searches (the gallery downloader page) run from
`hydrus gallery <store> --downloader <name> <queries>`, up to the file
limit. All four kinds are checked against the reference end to end.
Tracked as task "Downloader engine"; what's left is migrating custom
bandwidth rules, and the GUI.

## Goal

Your existing downloaders keep working after migrating: the URL classes,
parsers, gallery URL generators (GUGs) and login scripts in your database, the
subscriptions with their history, and the watchers and URL queues you have
open. On top of that: Hydrus Companion's `/add_urls/add_url`, and downloads
that are faster and easier to follow than the reference's.

"Keep working" is checked the same way as everything else: by running the
reference on the same inputs and comparing.

## What the reference does (for orientation)

About 40,000 lines of Python in `hydrus/client/{parsing,networking,importing}`:

| part | reference | what it is |
|---|---|---|
| parsing | `ClientParsing.py`, `ClientParsingResults.py` | page parsers → content parsers → formulas (HTML, JSON, regex, compound, context variable, nested) → string processors; produces URLs, tags, notes, hashes, times, titles, vetoes, next-page links |
| network | `ClientNetworking*.py` | one job per request: bandwidth rules per network context, per-domain connection limits, cookies and headers per session/context, login scripts, retries |
| importing | `ClientImport{FileSeeds,GallerySeeds,SimpleURLs,Gallery,Watchers,Subscriptions}.py` | queues of *file seeds* (a URL or path to import, with status and note) and *gallery seeds* (a page of search results); URL lists, gallery searches, thread watchers and subscriptions drive them |

Its HTML parsing uses html5lib, i.e. the WHATWG HTML parsing algorithm; a
Rust HTML5 parser (html5ever) builds the same trees, even for broken pages.

## Design

Three new crates, each testable alone:

- **`hydrus-parse`**: the parsing engine, pure functions from (definition,
  document, context) to results. The definitions are native Rust types
  converted from the reference's serialised objects at import (the URL class
  and string processing types in `hydrus-core::url` are the start of this).
  No network, no database.
- **`hydrus-net`**: an async HTTP engine (tokio + reqwest). Jobs carry a
  network context chain (global → domain → downloader → subscription) that
  decides bandwidth, cookies (`hydrus-store::network`, done) and headers
  (done). Per-domain concurrency limits and backoff are explicit and visible,
  not hidden waits.
- **`hydrus-download`**: import queues. A queue is a list of items (file
  seeds and gallery seeds) in native tables, so thousands of subscription
  entries are rows, not a serialised blob rewritten on every change. Kinds:
  URL list, gallery search, watcher, subscription query. Workers pull items,
  fetch (hydrus-net), parse (hydrus-parse), import (hydrus-import, done) and
  record the outcome per item.

Until there is a GUI, a reference *page* (where Companion's
`destination_page_name` sends URLs) becomes a **named queue**; the GUI will
show named queues as pages.

## Migration

| reference | becomes |
|---|---|
| URL classes and parser links | done (`url_classes` setting) |
| parsers, GUGs/NGUGs, login scripts | native definitions; anything that fails to convert is kept verbatim and reported |
| default import options per URL class / downloader | settings |
| cookies, custom headers | done |
| bandwidth rules (and recent usage) | settings (+ usage rows) |
| subscriptions: queries, their file seed caches and gallery logs, check timings | done (`subscriptions` and `subscription_queries`, each query's history an import queue); old-style import options on subscriptions last saved before hydrus v670 are converted as the reference converts them |
| watchers and URL/gallery pages open in the GUI session | named queues |

## Testing

1. **Formulas and parsers, randomised against the reference.** An oracle
   script builds random HTML/JSON documents and random formula definitions
   with the reference's own classes, records what the reference extracts, and
   `hydrus-parse` must extract the same. Then whole page parsers (with
   sub-page splitting and content parser options) the same way.
2. **Your definitions.** Your database's parsers could be exercised on saved
   example pages. (None supplied: generated pages only.)
3. **End to end against a local site.** `oracle/record_downloads.py` serves
   a small fake booru/imageboard over localhost in three phases, gives a
   fresh reference client downloaders for it (URL classes, parsers, a GUG),
   and has it run a URL page (via `/add_urls/add_url`), a watcher (until
   the thread 404s), a subscription (two syncs) and a gallery search,
   recording every file
   and gallery log entry and what each file got.
   `crates/hydrus-api/tests/oracle_downloads.rs` installs the reference's
   domain manager as a migration would, replays the same steps through our
   Client API and downloader, and compares. **Done**; it matches. No real
   website is ever contacted by tests.

## Phases

1. `hydrus-parse`: formulas and string processing, then page parsers, then
   migrating parser definitions and GUGs. Randomised oracle. **Done.**
2. `hydrus-net`: HTTP jobs with cookies, headers, retries, ranged
   downloads and the reference's default pacing (one request a second per
   site, five overall); local test server. **Done**, except migrating
   custom bandwidth rules and proxies.
3. `hydrus-download`: import options (migrated, layered as the reference
   layers them), queue tables, the URL worker, URL queues and
   `/add_urls/add_url`. **Done.** Then gallery pages (including gallery
   URLs sent to URL queues) and GUG searches; watchers.
4. Subscriptions and migrating them with their history. **Done**:
   migration (oracle-tested: every saved version of seeds and
   subscriptions, old-style import options of every version the reference
   upgrades from), the reference's check timing and history compaction
   (oracle-tested), syncing (the reference's "caught up" and file limit
   rules) and file work, run on schedule; bulk-adding queries from the
   command line. End to end against a local booru; the comparison with the
   reference on the same site is task "End-to-end downloader oracle".

Login scripts are not planned: logins come from cookies (Companion sends
them), which already migrate and work. A migrated downloader that names a
login script is reported, and its requests go out with the domain's cookies
as usual.

## Decisions (from the questions in issue #1)

- **Sites:** mainly boorus; Safebooru is the example to test against. The
  local test site mimics a Gelbooru-0.2-style booru (gallery pages of
  thumbnails, post pages with the file link and tags in the sidebar), driven
  by parsers of that shape built with the reference's classes.
- **Kinds:** all of them: single URLs, gallery searches, watchers and
  subscriptions.
- **Subscriptions:** about 200, so migrating them with their history (what
  was already seen, so nothing is fetched again) is essential, and checking
  them must be cheap: queue rows, not a serialised blob rewritten per
  change. Adding many at once was clunky in the reference; here a list of
  queries (one per line) becomes subscriptions in one step, with the
  downloader and options chosen once, and duplicates of existing queries are
  skipped and reported.
- **Logins:** cookies only (see above).
- **Test pages:** generated pages only.
- **Companion pages:** a named queue per page name.
