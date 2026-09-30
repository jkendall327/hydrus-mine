# Downloader design

Status: design, not yet built. Tracked as task "Downloader engine".

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
| subscriptions: queries, their file seed caches and gallery logs, check timings | queue rows |
| watchers and URL/gallery pages open in the GUI session | named queues |

## Testing

1. **Formulas and parsers, randomised against the reference.** An oracle
   script builds random HTML/JSON documents and random formula definitions
   with the reference's own classes, records what the reference extracts, and
   `hydrus-parse` must extract the same. Then whole page parsers (with
   sub-page splitting and content parser options) the same way.
2. **Your definitions.** Your database's parsers can be exercised on saved
   example pages, if you can supply some (see the open questions).
3. **End to end against a local site.** The oracle serves a small fake
   booru/imageboard over localhost, points the reference's downloaders (URL
   classes, parsers, GUG) at it, runs a URL import, a gallery search, a
   watcher and a subscription, and records what ends up in its database. We
   run the same against the same site and compare. No real website is ever
   contacted by tests.

## Phases

1. `hydrus-parse`: formulas and string processing, then page parsers, then
   migrating parser definitions. Randomised oracle.
2. `hydrus-net`: HTTP jobs with cookies, headers, bandwidth; local test server.
3. `hydrus-download`: URL-list queues and `/add_urls/add_url` (completes
   Companion support); then gallery searches and GUGs; watchers.
4. Subscriptions and migrating them with their history.
5. Login scripts, if needed (see questions).

## Open questions

Asked in a GitHub issue; defaults in brackets are what happens without an
answer.

- Which sites do you download from, and which kinds: single URLs from
  Companion, gallery searches, thread watchers, subscriptions (roughly how
  many)? [all kinds, subscriptions last]
- Do any of your downloaders need a login script? [assume not; login scripts
  last]
- Could you share a few saved pages (HTML/JSON) from your main sites, for
  testing your own parsers? [test with generated pages only]
- Until the GUI exists, is a named queue per Companion "page" name fine?
  [yes]
