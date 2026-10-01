# GUI plan

The last phase of the roadmap (DECISIONS.md): the desktop client, in Slint.
This is the map of the reference's Qt GUI (v688) that the work starts from,
and how we mean to go about it.

## Where it stands

`hydrus-gui <store>` opens the last session (the one a migration brings
over from hydrus), or a single search page if there is none; if you had a
lock password set in hydrus, it asks for it first, as hydrus does, and
cancelling closes it without opening anything. Its windows
follow the system's light or dark mode (hydrus's own colour options aren't
carried over yet). Each notebook
on the way to the page shown has a row of tabs, and a notebook opens on its
first page, as in the reference. A search page opens as it was left: its
search, its sort and the files it showed, not searched again until its
search changes (and then only if it is synchronised). Downloader pages and
the pages we don't open yet show their files, and say what they are.
Changing a page's sort sorts the files it shows rather than searching
again, as the reference does. Ctrl+T or F9 opens the reference's page
chooser, nine buttons laid out as a number pad (the digits, arrows and
enter press them): file search, then a file domain, opens a search page
on it; special opens a page of pages or a duplicates page; downloader
pages can't be made here yet. With other saved sessions (those hydrus had,
say), a "sessions" button lists them, and choosing one appends a copy of
its pages in a page of pages named after it, as the reference's "append
session" does; a saved session's downloader pages say their downloads
don't run. The new page goes at the far right of the
current notebook, and Ctrl+W or a middle click on a tab closes it, the next tab to
the right (or left) being shown, as in the reference; downloader pages
can't be closed yet, since their queues would run on without them. The
pages are saved as the last session
every five minutes and on exit, as the reference saves them: each page
opened with its search, sort and files, the others as they were.

On a search page, at the top of its sidebar, the
sort control offers every sort type named and ordered as in the reference,
each with its two orders ("oldest first", "newest first"...); choosing a
type picks its default order, as the reference does. With nothing typed, the search
box offers `system:everything`, `system:inbox` and `system:archive` with how
many files each finds in the page's file domains, counted as the reference's
`_GetFileSystemPredicates` does (the reference also offers the system
predicates that open an editor; not yet). Typing in the search box lists
the matching tags with their counts (display tags, in the page's file domains
and tag service; the exact match first, then the most used), up and down move the
highlight, and enter adds the highlighted tag, or the text as typed for a
system predicate; a leading hyphen excludes. Predicates are listed as the
reference writes them (`system:width>1920` shows as `system:width > 1,920`),
and double-clicking one removes it. Below them, as the reference's
"selection tags" box, the tags of the selected file (or with nothing
selected, of every file on the page) with how many have each (`tag (3)
(+1)` for pending, `(-1)` petitioned), display tags in the page's tag
domain less those your tag display filters hide from it, sorted by your
search page tag sort and namespace order (checked against the reference's
`SortTags`); double-clicking a tag searches for it too. Tags here, in the
autocomplete and in the search's predicates are shown as your options
have them (namespaces shown or hidden, the namespace connector,
underscores and emojis replaced; checked against `RenderTag`). The ★ beside
the search box lists your favourite searches in hydrus's folders and
order; choosing one loads its domains, tag service, predicates and sort
into the page, searching if it is synchronised (saving and editing
favourites isn't here yet, nor collecting). The matching files' thumbnails fill the grid, newest import
first, with the count in the status bar; the grid is a list of rows, so only
the rows in view exist, and a thumbnail is read and decoded off the UI
thread when its row first comes into view (a blank frame until then). Double-clicking a thumbnail opens the media viewer in its own
window on that file, fitted to the window: right and left (or page down and
up, or the mouse wheel) move through the page's files, round from the last
to the first as in the reference, and escape closes it. Images are shown
whole. Video, audio and animations play in mpv, as the reference's defaults
have it (animated WebP and JPEG XL, and ugoiras, show their first frame
until there is a native animation player): libmpv is loaded when first
needed, so building needs nothing more, and without it these show their
thumbnail. A file loops, space pauses it, and the store's `mpv.conf` (else
hydrus's default one) applies. Frames come from mpv's software renderer, on
a thread of their own (`src/mpv.rs`); that is slower than the reference's
embedded mpv window for large videos. Other files show their thumbnail.
A duplicates page (a migrated session's, with its search, pair sort and
group mode) says how many potential pairs its search finds, and launches
the duplicate filter: its own window, showing one file of a pair at a time
(left and right, or the mouse wheel, switch to the other) beside the
reference's comparison statements and score (`hydrus-duplicates`'s port,
the slow ones, jpeg quality and visual duplicates, made off the UI thread)
and the decisions: this is better (deleting the other or not), same
quality, alternates, not related, skip, go back. As in the reference, a
left click on the file is "better, delete the other", a right click
"alternates", a middle click goes back and up skips; video, audio and
animations play as in the media viewer, and the next three pairs' files
are decoded ahead. Pairs come a batch
at a time (`duplicate_filter_max_batch_size`), or a group at a time in
group mode; pairs with a file already merged away or deleted are skipped;
decisions wait for the batch's end, which asks to commit them (unless
they are few, as `duplicate_filter_auto_commit_batch_size` has it), and
closing with decisions pending asks too. Merges use your duplicate merge
options. The model is `src/duplicate_filter.rs`, tested in
`tests/duplicate_filter.rs` with the window drawn headless.
`crates/hydrus-gui/tests/search_page.rs` drives the page and
`tests/session.rs` a saved session, and both draw the window headless (the
screenshots land in `target/tmp/`). Not yet: reopening closed pages, system
predicates in the autocomplete, collect, the viewer's hover frames, seeking
and volume, a native animation player, downloader pages' own panels; in
the duplicates page, editing its search and the preparation and
auto-resolution tabs; in the duplicate filter, the custom action,
deleting from the filter, the hover frames, zooming and panning, and
reviewing auto-resolution's pending pairs.

## Size

The reference GUI (`hydrus/client/gui`) is 225 files, about 179k lines of
Python:

| area | lines |
|---|---|
| main window, menus, popups, subscriptions, shortcuts, string editors | 36k |
| media viewers (canvas, hover frames, media containers) | 17k |
| downloader definition editors (URL classes, parsers, GUGs, logins) | 16k |
| metadata (manage tags/URLs/notes/times, sidecars) | 14k |
| subscriptions, import/export folders, import options | 13k |
| thumbnail grid (and its menus) | 13k |
| options (38 pages) | 12k |
| other panels | 11k |
| lists (tag lists, multi-column lists) | 11k |
| search (autocomplete, predicate editors) | 9k |
| page notebook and sessions | 9k |
| duplicates (filter, auto-resolution UI) | 8k |
| widgets, media menus, page sidebars | 15k |

About 10k lines are for things not wanted (petitions, repositories, the
hydrus server, IPFS).

## The reference's structure

**Main window** (`ClientGUI.py`, `FrameGUI`): a notebook of pages (optionally
with a page tree), a status bar, popup messages in the corner (downloaders
and subscriptions report through them), and menus: file, undo, pages,
database, network, services, tags, pending, help.

**Pages** hold a sidebar (a "management panel"), a preview viewer under it,
and a thumbnail grid. Page types: file search, gallery downloader, URL
downloader, watcher, simple downloader, import from disk, duplicates, and
pages of pages. A page's state is a `ManagementController` (a name, a type
and a dictionary of variables such as its file search or its importer).

**Sessions** are trees of pages saved in the database: containers in
`json_dumps_named` (`GUI_SESSION_CONTAINER`, with timestamped backups) and
each page's data in `json_dumps_hashed` (`GUI_SESSION_PAGE_DATA`, keyed by
its hash, so only changed pages are saved again). "last session" is saved
every five minutes. hydrus-rs already carries these over verbatim when
importing a legacy database.

**Thumbnail grid** (`ClientGUIMediaResultsPanel*`): owns sort, collect,
selection and focus; paints cached thumbnail pages with overlays (borders,
icons, ratings, tag banners). Its right-click menu is large (about 3.7k lines
with its actions).

**Tag autocomplete** (`ClientGUIACDropdown.py`): a text box with a floating
dropdown (results, children, favourites), the active predicate list, and
location and tag-domain buttons. It fetches off-thread and filters narrowing
input from a client-side cache. About 50 system predicate editors.

**Media viewer** (`canvas/`): a separate window; hover frames on each edge
(tags, ratings and locations, notes, duplicates); static images rendered in
tiles; animations either by the native renderer (ffmpeg frames) or mpv
embedded; an archive/delete filter and the duplicate filter as viewer modes.

**Duplicate filter**: the duplicates page has preparation (similar-files
search), filtering (the potential-duplicates search, sort, group mode, merge
options, launch) and auto-resolution tabs. The filter shows A and B in turn
with comparison statements (resolution, size, mime, tags, times, jpeg
quality, pixel and visual duplicates...) and buttons or shortcuts for each
decision. Decisions are queued per batch and committed at the batch's end.
What makes it slow in the reference: after every commit it re-reads the
whole search space; it fetches batches through repeated "fragmentary" reads
throttled to half a second each; it commits four decisions per round trip.

**Dialogs the owner's features need**: manage subscriptions and edit
subscription; import folders; export folders; sidecar editors (with string
processors); import options; manage tags, URLs, notes and times; the options
dialog; the downloader definition editors; auto-resolution rules.

**Shortcuts**: named shortcut sets (`SHORTCUT_SET`) mapping keyboard and
mouse shortcuts (in hydrus's own key codes) to application commands (about
200 simple commands plus content commands), with eleven reserved sets
(global, main window, thumbnails, media viewer, duplicate filter...).

**GUI state in the database**, to carry over: client options (window
positions, viewer settings per mime, default sort and collect, colours,
thumbnail settings), column widths, favourite searches, tag display and
autocomplete options, sessions, shortcut sets, recent tags.

## How we mean to build it

1. **A page and session model in the backend first**, independent of any
   GUI: a tree of pages whose state (a search and its results, an importer)
   lives in `hydrus-store`, decoded from the legacy session objects. The
   Client API's `/manage_pages` endpoints then answer from it, and the GUI
   renders it.
2. **View models in plain Rust, views in Slint.** Each screen's behaviour
   (what a click or a key does, what is selected, what is fetched) lives in
   a testable Rust type; Slint files only lay out and bind. Behaviour is
   tested without rendering; layout is checked with screenshots from Slint's
   software renderer, which runs headless.
3. **Order, by the owner's use**: a search page (autocomplete, thumbnail
   grid, sort and collect) and the media viewer; then downloader pages
   (gallery, URL, watcher) and popups; then the duplicate filter, built to
   commit decisions as they are made and to fetch pairs without re-reading
   the whole search space; then the dialogs for subscriptions, import and
   export folders, sidecars and manage tags; then options, shortcuts and
   the downloader editors.
4. **Fidelity**: the same page types, menus, shortcuts and dialogs, reading
   the same stored state, so a migrated user finds their session and
   settings as they left them. The one planned redesign, a more ergonomic
   duplicate filter, comes after the reference's behaviour is matched.

## Decisions

- **Video and animation: embed mpv** (as the reference can), for now; the
  owner's choice. Done, through mpv's software renderer. Decoding with
  ffmpeg and drawing frames ourselves (the reference's native renderer) may
  come later.

## Open questions

- Slint is to be tried first (DECISIONS.md); the first milestone, a search
  page with a thumbnail grid over a real library, is where that is judged.
