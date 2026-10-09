# Decisions

Open questions for the maintainer go in GitHub issues. This file keeps the
record of what was decided, and why the roadmap looks the way it does.

## Decided (2026-09-29)

- **GUI: Slint.** To be tried when the GUI work starts. The backend and
  Client API come first either way.
- **Name: hydrus-rs**, with the binary called `hydrus`.
- **Client API over HTTP only.** No HTTPS.
- **Features in use, which set the priorities:**
  - used: downloaders; duplicates filter (would be used more if it were
    faster); import and export folders; sidecars; Hydrus Companion (in
    Vivaldi) through the Client API
  - not used: the PTR and other tag repositories; the hydrus server; file
    repositories; IPFS

## Decided (2026-10-01)

- **A rich daemon and a thin GUI** ([#24], [#25]). The real work runs in
  the daemon, `hydrus serve`: downloads, subscriptions, watchers, import
  and export folders, maintenance and the Client API. The GUI shows and
  controls what is in the store, and runs no work of its own. The
  condition is that using it feels as hydrus does, so opening the GUI is
  enough: it starts the daemon when none is running for the store, and
  stops it on closing. A daemon started on its own (as a service, say)
  runs on. Why: after parity, a web UI is likely to be built on the same
  daemon, so behaviour belongs in the daemon and in plain Rust, not in
  Slint.
- **Pages live in the store.** The GUI saves its pages as they change, and
  `/manage_pages` answers from the store whether or not the GUI is open;
  the endpoints that act on pages (focusing one, adding files, refreshing)
  pass through the store to the GUI.
- **Downloader pages are views over the daemon's queues**: their progress,
  and controls (adding URLs and queries, pausing, retrying), with the
  daemon doing the downloading.

[#24]: https://github.com/jkendall327/hydrus-mine/issues/24
[#25]: https://github.com/jkendall327/hydrus-mine/issues/25

## Decided (2026-10-02)

- **GUI order, by the owner's use now that it runs on their library:**
  1. the main window: its frame (title, size and place kept, the status
     bar) and the menu bar;
  2. popup messages;
  3. settings (the options dialog);
  4. opening every kind of page, downloaders included, from the menus as
     well as the page chooser;
  5. the downloader pages' remaining parts;
  6. importing files (the import page, files dropped on the window);
  7. the search page's daily-use gaps (system predicate editors, file
     domain and tag service buttons, thumbnail overlays, saving
     favourites).

  Everything else (the duplicates page's other tabs, metadata editors,
  managing subscriptions and folders in the GUI, shortcuts, downloader
  editors, the simple downloader, dragging files out) waits until these
  are done.

## Decided (2026-10-08)

- **Verification is a fast local loop, CI on every push, and an honest test
  tag per leaf**, replacing the per-leaf ledger and publication checkpoints
  (history: `docs/rust/history/2026-10-08-workplan.md`). A leaf is done when a
  test tagged with its ID passes on `master` (`docs/rust/tracking/README.md`).
- **Linux only** for now; Windows and macOS CI run by hand.
- **Remaining work lives in GitHub issues**, one PR per issue with one
  independent review, merged to `master` when green. No long-lived
  integration branches.
- **Tests replay recordings of the reference** wherever behaviour is more than
  strings; `scripts/setup-oracle.sh` makes the reference runnable anywhere.
- **The generated UI crate stays whole for now.** Splitting it by window group
  would cut a `.slint` rebuild from ~7 minutes to ~1-2, at the cost of a day or
  more of churn (shared structs and five Slint globals used across groups).
  Agents in separate containers removed the queueing that made rebuilds
  hurt; issues #86 and #91 log rebuild time to decide whether the split is
  still worth it.

- **Out of scope (2026-10-08, #95)**, each recorded on its leaf in
  `docs/rust/tracking/leaves.json` (priority `out-of-scope`, first note says
  why; undo by setting it back to `normal`): background sibling/parent loading
  (`siblings-async`, `parents-async`), the background display-sync manager and
  its two Tags > sync idle/normal-time switches,
  repository tabs in Manage Tags, "prefer system FFMPEG", the self-sizing
  viewer's rescue padding, locale integer rendering, the three image
  tile-cache settings, the toaster's mouse-on-another-display freeze, two Qt
  window-state tray workarounds, and three Help > debug entries for the
  Python thread pool and Qt canvas tiles. `scripts/track.py` stops counting
  them.
- **The About box lists hydrus-rs's own components** (platform, SQLite,
  optional libraries such as ffmpeg and mpv) as the equivalent of the
  reference's Python/Qt/numpy lines.
- **System tray: wanted**, through Slint 1.18's own `SystemTrayIcon` (the
  `system-tray` feature: a pure-Rust D-Bus StatusNotifierItem on Linux, not
  Qt). Minimise-to-tray cannot detect minimising on Wayland; that part is
  X11-only.
- **Merging:** the coordinating agent may merge a PR once CI is green and its
  independent review's findings are addressed.

- **Date parsing matches a hydrus install without `dateparser`** (2026-10-08).
  The reference's "datestring to timestamp (easy)" and its other `ParseDate`
  callers use the optional `dateparser` library when installed and fall back
  to `dateutil` otherwise. hydrus-rs is a superset of the `dateutil`
  fallback (recorded from the reference with `dateparser` disabled) that adds
  English relative dates ("now", "yesterday", "2 hours ago", "in 3 days"),
  giving the same result `dateparser` would for those. It does not take on
  the rest of `dateparser`: non-English and fuzzy free-text dates are not
  parsed, and the advanced strptime step covers explicit formats.

- **Deferred, low priority (2026-10-08)**, each noted on its leaf: drag-out
  of files and its three exporting options (wanted, but it needs
  hand-written X11/Wayland drag sources); saved size and position for the
  remaining ~25 windows; idle from mouse movement outside hydrus-rs windows
  (later through the platform's idle time, not cursor polling); custom SVG
  rating stars; clipboard-bitmap and PNG drag-and-drop downloader exchange.
  Reading old saved-object versions stays in scope: importing an existing
  hydrus install must be easy.

- **Client API extras are wanted** (2026-10-08, #120): HTTPS, the
  normie-friendly welcome page, and the external scheme/host/port overrides.
  The reference hides the three override rows (`if False:`) and nothing reads
  them; hydrus-rs shows them as editable rows in advanced mode at the owner's
  request. The video buffer follows the reference's sizing (#121).

## Roadmap that follows

1. **Client API parity**, with Hydrus Companion's request patterns checked
   first: URL lookups (`/add_urls/get_url_info`, `/add_urls/get_url_files`),
   sending URLs (`/add_urls/add_url`), tags, pages, cookies and headers.
2. **Search and local import**: file search, autocomplete, the media
   pipeline, the file import pipeline.
3. **Duplicates**, built for speed: perceptual-hash search, potential pairs
   and the filter's data model.
4. **Downloaders**, redesigned rather than ported: gallery downloaders,
   subscriptions, watchers, URL classes, parsers. `/add_urls/add_url` needs
   this.
5. **Import/export folders and sidecars.**
6. **GUI in Slint.**

Deprioritised: tag repository sync, the hydrus server, file repositories and
IPFS. Their data and settings are still imported verbatim, so nothing is lost
if they're wanted later.

**Fidelity first.** The goal is an accurate copy of hydrus as a trustworthy
base; changes to how things behave come later, on top of it. The owner's
own use (duplicates at distance 0 and 2, auto-resolution rules, subscriptions,
watchers, an import folder as a drop box, a symlinking export folder) sets
priorities, never semantics. The one planned redesign, a more ergonomic
duplicate filter, comes after the reference's filter behaviour is matched.
