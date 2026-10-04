# GUI plan

The last phase of the roadmap (DECISIONS.md): the desktop client, in Slint.
This is the map of the reference's Qt GUI (v688) that the work starts from,
and how we mean to go about it.

## Where it stands

`hydrus-gui <store>` opens the last session (the one a migration brings
over from hydrus), or a single search page if there is none, its tabs named
as hydrus names them (each page's name, elided to the longest hydrus's
options allow, with its number of files and an importer's progress, a
notebook's decorated: "url import (5 - 6/10)", "pages (60) ↓"); if you had a
lock password set in hydrus, it asks for it first, as hydrus does, and
cancelling closes it without opening anything. Once open, it starts the
daemon, `hydrus serve`, which does the work (downloads, subscriptions,
import and export folders, maintenance and the Client API), unless one is
already running on the store; it looks for the `hydrus` program beside
itself, then on the path. Closing the client stops the daemon it started
(giving it 20 seconds to finish what it is doing), as hydrus's work stops
when it closes; one started on its own (as a service, say) runs on, and if
that one stops, the client starts its own. If the client's daemon can't
start or stops by itself, a line above the status bar says why (its own
error: a media location missing, say) with a button to start it again; if
the daemon runs but its Client API couldn't start (its port in use), the
line says that, as hydrus's popup does ("Could not start "client api":
...").
The client keeps its pages in the store as they change (within half a
second: the session, the page shown, each page's files and selection, and
the media viewer and its file), so the Client API's `/manage_pages`
answers from it, and does what that asks of the pages (showing one, adding
files to one, refreshing one or a notebook's) the next time it looks. It
opens on the page it showed last, and only one client opens a store at a
time.
The main window opens
where and as big as hydrus had it (maximised, by hydrus's default) and
keeps its size and place as it closes, as hydrus's frame locations do
(less hydrus's fitting of a window to its screen). Its windows
follow the system's light or dark mode (hydrus's own colour options aren't
carried over yet). Each notebook
on the way to the page shown has a row of tabs, and a notebook opens on its
first page, and on the page it showed last when you come back to it, as in
the reference. As the reference's main window shortcuts have it, ctrl+page
up and down show the page beside (in the deepest notebook that can move,
unless a notebook above moved in the last three seconds, so a held key
runs along the top tabs), f5 searches the page again (resuming a paused
search; a locked one stays), and ctrl+s and ctrl+m give the keyboard to
the search box and the thumbnails. A search page opens as it was left: its
search, its sort and the files it showed, not searched again until its
search changes (and then only if it is synchronised). The pages we don't
open yet show their files, and say what they are; URL, gallery and watcher
downloader pages are live (below).
Changing a page's sort sorts the files it shows rather than searching
again, as the reference does. Ctrl+T or F9 (or, as in the reference, a double click,
left or middle, on a tab row's empty space, the new page then going in
that row's notebook) opens the reference's page
chooser, nine buttons laid out as a number pad (the digits, arrows and
enter press them): file search, then a file domain, opens a search page
on it; special opens a page of pages or a duplicates page; download, then
urls, gallery or watcher, opens that downloader page (a simple
downloader page can't be made here yet). With other saved sessions (those hydrus had,
say), a "sessions" button lists them, and choosing one appends a copy of
its pages in a page of pages named after it, as the reference's "append
session" does; a saved session's downloader pages say their downloads
don't run. The new page goes at the far right of the
current notebook, and Ctrl+W or a middle click on a tab closes it, the next tab to
the right (or left) being shown, as in the reference; closing a
downloader page that is still importing, or holds anything, asks first,
as the reference does ("This page is still importing."), and a closed
page's queues wait. Ctrl+U
reopens the page closed last (within the hour, as the reference keeps
them), where it was and as it was, and shows it. The
pages are saved as the last session
every five minutes and on exit, as the reference saves them: each page
opened with its search, sort and files, the others as they were.

Popup messages show at the bottom right, as the reference's popup
message manager shows them (`src/popups.rs`): the oldest ten in the
queue the daemon and the Client API add to (in the store), each with its
title, texts, progress gauges, the download it is doing (a subscription's
page or file, as a downloader page shows one), a button to show its files
in a new page
(named for them), its error's traceback, and buttons to pause or cancel
it while it runs; a right click dismisses one that is done, and the line
under them counts them, with "dismiss all" (those done) and an arrow to
hide or show them. They update four times a second.

The status bar at the bottom says, as the reference's does, the page's
status (its files, or the selection's) and, on the right, the network's:
what the daemon has read since the client opened, what it is reading a
second, and whether subscriptions or all new network traffic are paused
("12.3 MB (45 KB/s), subs paused"); the daemon says what it has read four
times a second.

Above the tabs is the reference's menu bar (`hydrus-gui-model/src/main_menu.rs`, checked
against the bar the running reference shows on the fixtures, recorded by
`oracle/record_main_menu.py`): file, undo, pages, database, network,
services, tags, pending (with repositories) and help, every entry hydrus
has in its place, enabled and ticked as hydrus shows it. What hydrus-rs
can't do yet is greyed out. A menu opens on a click (or alt and the
letter hydrus gives its title: alt+f for file), submenus open as the
pointer reaches them, and the arrows, enter and escape move through them
as Qt's do; a press anywhere else closes them. What works so far:

- file: pausing import and export folders, checking an import folder or
  running an export folder now, opening the installation and database
  directories, the options, and exit;
- undo: the pages closed in the last hour, latest first, to reopen any of
  them, or forget them all (asking first); with none, the menu is greyed
  out, as hydrus's is;
- pages: how many pages are open and the session's weight (files, and
  twenty for each download's item or search); the history of pages shown,
  latest first, to show one again; refresh; appending a saved session,
  saving the open pages as one (`hydrus-gui-model/src/session_saving.rs`,
  checked against the reference, recorded by
  `oracle/record_sessions_menu.py`: "as new session…" asks a name, refuses
  "last session", "exit session" or "just a blank page" with a warning
  and asks again, and asks before overwriting one that exists, "no,
  choose another name" asking again; a session's own entry asks whether to
  overwrite it; a session saved is a copy of the pages, with their files,
  so it stays as saved while the pages change), "clear and load" (asking
  first, then, if a downloader page objects, asking again with the
  reference's words for every page's objection; the pages closed are
  gone, not kept to reopen, and the session's pages are at the top), and
  deleting one (asking first); the page chooser; a new search page on each
  local file domain, the trash or a file repository (searching the tag
  service hydrus's options give search pages, as each new search page
  does, and all the files stored here rather than all known files with
  every tag service); new URL, watcher and
  gallery pages, a page of pages and a duplicates page; and clearing every
  watcher page's highlight;
- database: whether file maintenance works in idle and normal time;
- network: every pause switch hydrus has (all new network traffic,
  subscriptions, all paged importer work, file importing, gallery
  searching, watcher checking), which the daemon obeys; in advanced
  mode "nudge subscriptions awake", which wakes `hydrus serve`'s
  subscriptions daemon to look for subscriptions due; and
  "subscriptions…", the manage subscriptions dialog (below);
- tags: the siblings and parents editors (below);
- pending: each repository's content to upload, and forgetting it (asking
  first);
- help: the help, links and changelog in the browser, and advanced mode
  (which adds hydrus's advanced entries).

Network > "subscriptions…" opens the manage subscriptions dialog
(`src/subscriptions_window.rs`, `hydrus-gui-model/src/subscriptions_dialog.rs`),
the reference's `EditSubscriptionsPanel`. It lists the subscriptions as
the reference does (name, source, status such as "2 working, 1 paused,
1 dead", last new file time, last checked, error/delay such as
"delayed--retrying in 2 hours - because: …", items, paused), sorted by
any column and selected as the reference's lists select, and warns when
subscriptions are paused from the network menu. Its buttons delete the
selected (asking "Remove all selected?"), duplicate them ("name (1)",
with copies of their queries' file logs), merge those sharing a
downloader (asking which is primary and its new name), separate one (in
half, into a subscription a query, named "base: query", or only some
queries, ticked in a list, into one new subscription or one each), lowercase
their queries' texts, deduplicate queries with the same text on the same
downloader (asking, as the reference does, whether to match case, which
downloader, which texts, ticked in a list, and which subscription keeps
them, which keeps its query with the most files; the others lose theirs;
`oracle/record_subscriptions_dedupe.py`), pause or resume each, scrub their delays, check
their queries now (asking first, as the reference does, whether to
unpause paused subscriptions, check DEAD queries, and unpause paused
queries), retry their failed or ignored files, reset them (emptying their
queries' file logs), select the subscriptions with a query containing
some text, and overwrite the selected's downloader or checker options
(their queries' check times reckoned again). Nothing is written until "apply", which writes
only what changed; "cancel" writes nothing. The rows and the questions
are as `oracle/record_subscriptions_list.py` recorded the reference's.

"add" opens a separate gallery list, even with one configured downloader,
then the subscription editor. The chooser reads the current saved downloader
definitions and preselects the saved default; cancelling adds nothing. With
no definitions it shows a separate warning. "overwrite downloader" uses
the same list. `oracle/record_subscription_add.py` records the real Qt
chooser and editor flow; GUI regressions save both native windows. "edit"
(or a double-click) open the edit subscription dialog
(`src/edit_subscription_window.rs`, `hydrus-gui-model/src/edit_subscription.rs`),
the reference's `EditSubscriptionPanel`: the name, the delay line, the
downloader (a button to choose another), the queries list and its
buttons, "currently paused", the file limits, "do not worry about
subscription gaps", the checker options (the checker options editor),
and the file publication options. The queries list's buttons add a query
and edit one (in the query editor: query text, display name, check now,
paused, and the file and search logs' status), copy and paste queries
(the paste asking what to add and whether to revive the DEAD, as the
reference words it), delete, pause/play, retry failed and retry ignored
(all, 403s, 404s or blacklisted), check now (asking the "Check which?"
questions) and reset (asking first). "apply" gives the subscription back
to the list, renamed " (1)" if its name is taken; the list's "apply"
writes it, with new queries, deleted queries and the file logs' resets
and retries. Its workings are as `oracle/record_edit_subscription.py`
recorded the reference's.

A downloader page's "imports" box (a URL downloader's, a local import's,
and the highlighted search's or watcher's) has a "file log" button, which
opens the file log window (`src/file_log_window.rs`,
`hydrus-gui-model/src/file_log.rs`), the reference's
`EditFileSeedCachePanel`: the importer's files as the reference lists
them (#, source, status, added, last modified, source time, note) with
its status above. A right click on files (selecting the one under the
pointer) offers opening their files in a new page, copying their URLs or
paths and notes, opening them, their hashes, URLs, headers and tags, trying
them again, skipping and deleting them (asking first), and the whole log's
menu, which the "whole log" button also opens: retrying failures and
ignored files, deleting files of each status, skipping the unstarted,
showing the new or all files in a new page, reversing the order, and
copying every source. Its actions change the importer at once, as the
reference's frame does. The rows and menus are as
`oracle/record_file_log.py` recorded the reference's. Windows' own
popup menus are drawn as the menu bar's are (`src/popup_menu.rs`).

The "search" box (a gallery search's, a URL downloader's) has a "search
log" button, and a watcher's "checker" box a "check log" button, which
open the same window as the search log (`src/search_log_window.rs`,
`hydrus-gui-model/src/search_log.rs`), the reference's
`EditGallerySeedLogPanel`: the pages read (#, url, status, added, last
modified, note) with the log's status above. A right click on pages
offers copying their URLs and notes, opening them, the page's referral
URL, headers and inherited tags, trying it again (just that page, or a
search letting it carry on to next pages), skipping it, and the whole
log's menu: deleting entries of each status (asking first), restarting a
search whose last page failed, and copying every URL. Changes nudge the
downloader to work the queue again. As `oracle/record_search_log.py`
recorded the reference's.
The subscription query editor's "history" box has "file log" and
"search log" buttons for the query's logs (its search log is read only,
as the reference's is), and the edit import folder dialog a "file log"
button for its cached import paths.

The edit subscription and edit import folder dialogs' "import options"
button, a gallery or watcher page's (for the searches or watchers it
makes), and the "highlighted" box's (for that search or watcher alone,
under the highlighted search's own file limit), and a URL downloader's
or local import page's, opens the import options editor (`src/import_options_window.rs`,
`hydrus-gui-model/src/import_options_editor.rs`), the reference's
`EditSpecificImportOptionsContainerPanel`: a list of the kinds of options
(in simple mode, as a new client has it), each labelled with whose
default it uses ("default presentation (import folder)") or its custom
options' summary ("> presentation: presenting new files"), and the chosen
kind's page: "use the default import options" or "set custom import
options" (starting from the default's), and the options. Presentation,
prefetch (with its rule that the two checks can't both be dispositive),
file filtering's allowed filetypes (ticked in the reference's tree of
filetypes by group, a group's box ticking all of its) and its switches
and size and resolution limits, tag
filtering's blacklist and whitelist, locations' destination and
switches, tags (per tag service: getting tags, additional tags, and the
cog menu's switches; a warning if it gets no tags) and notes are edited
there. "apply" gives the dialog the
importer's options. As `oracle/record_import_options_editor.py` recorded
the reference's.

The tag filtering blacklist and each tag service's "get tags" filter
are buttons saying what the filter does ("blacklisting on goblin, orc",
"adding: all tags"), opening the reference's tag filter editor
(`ui/tag_filter.slint`, `src/tag_filter_window.rs`,
`hydrus-gui-model/src/tag_filter_editor.rs`) with the reference's
explanation: the filter as a whitelist ("allow these") and a blacklist
("exclude these"), each with boxes for unnamespaced tags, namespaced tags
and every namespace the client's parsers parse, a list (double-click or
"remove" to take entries out, asking first) and an input ("Series:*" is
the series namespace); and as its "advanced" rules, "exclude these" and
"except for these". A blacklist has the blacklist tab alone. It says
when an entry is already blocked or permitted by a broader rule, what the
filter does, and whether tags typed in the "testing" box pass (a
blacklist testing each with its siblings). What it shows and does at each
step is as `oracle/record_tag_filter_editor.py` recorded the reference's.
The shared favourites box saves named filters immediately, loads detached drafts,
asks before replacing or deleting a name, and imports/exports reference Tag Filter
JSON through clipboard text or a JSON file. Cancelling a pending name or import
leaves the draft and favourites untouched; cancelling the editor leaves the
caller untouched while keeping favourites already saved. These actions are
recorded by `oracle/record_tag_filter_favourites.py`. Each rule input has a
paste button for newline-separated slices; advanced mode offers a blacklist
"show other panels" button that reveals whitelist/advanced tabs while retaining
blacklist testing against siblings and namespaced tags.

A gallery or watcher page's list has the reference's right-click menu
(`src/importer_list_menu.rs`, `hydrus-gui-model/src/importer_menu.rs`):
a right press selects the row (if it isn't already), and the menu acts
on the selection. It offers copying queries (or a watcher's URLs and
subjects, and opening the URLs), "show files" (the importers' own
presentation, new files, inbox files, all files, or all files including
the trash, shown in the page with nothing highlighted), the file and
search (or check) logs (with one selected, each log's whole menu in a
submenu), retrying a watcher's failed and ignored files, removing, and
pausing or playing. As `oracle/record_importer_menus.py` recorded the
reference's.

File > import/export folders > "manage import folders…" and "manage
export folders…" open the folders dialogs (`src/folders_window.rs`,
`hydrus-gui-model/src/folders.rs`), the reference's
`EditImportFoldersPanel` and `EditExportFoldersPanel`: the folders listed
as the reference lists them, with add, edit (or a double-click) and delete
(asking first), and "apply" writing them ("cancel" writes nothing). An
added or renamed folder is named so no other has its name. The edit
import folder dialog has the folder's name, path, search subdirectories,
paused, check regularly and its period, the recent modified time skip
period, check on manage dialog ok, the popup and page options, and what
to do with each source file (delete, leave, or move, and where), each
folder typed or picked with "browse" (the system's folder picker); "apply"
refuses a folder with no path, inside hydrus's own directories, or moving
files nowhere, and warns of directories that don't exist, as the
reference does. The edit export folder dialog has the name, path, type
(regular or synchronise), trashing exported files (warned of, and asked
about on "apply"; never when synchronising), symlinks, the run period,
the popup, run on dialog ok, the query (typed as the Client API's tags,
double-click to remove), the filename phrase (refused if it doesn't
parse) and the sidecar overwrite options. The rows and the checks are as
`oracle/record_folders_lists.py` and `oracle/record_folders_dialogs.py`
recorded the reference's.

Both dialogs' sidecars button ("no sidecars", the one router as it
describes itself, or "3 sidecar actions") opens the reference's sidecar
editors (`ui/sidecars.slint`, `src/sidecars_window.rs`,
`hydrus-gui-model/src/sidecar_editors.rs`): the routers ("Taking from
.txt sidecar, applying some sorting, sending tags to media, on \"my
tags\".") with add, edit (or a double-click) and delete (asking first),
and for an export the templates menu's "easy one-click JSON that covers
the basics"; a router's sources, processing and destination, "apply"
asking first about notes split by newlines into or out of a .txt; and a
source's or destination's editor: "change type" (an import folder's
sources are .txt and .json sidecars and its destinations a file's tags,
notes, urls or timestamps; an export folder's the other way round),
keeping what two kinds share, the tag service and display type, the
timestamp type (with its file service, viewer or domain), the forced
note name, the JSON object names, the .txt separator, and the sidecar
filename (remove the file's extension, a suffix, and the resulting
sidecar for a test path). What they show and ask at each step is as
`oracle/record_sidecar_editors.py` recorded the reference's.

A router's or source's processing button (its steps, a line each) opens
the string processor editor (`ui/string_processor.slint`,
`src/string_processor_window.rs`, `hydrus-gui-model/src/string_editors.rs`):
its steps ("SPLIT: splitting by \",\"") with up, X (asking "Remove 1
selected?"), down, add (asking "Which type of processing step?") and
edit (or a double-click); and the test results, the starting strings and
those strings processed, and a single example through each step but the
slicers, a tab each ("splitter (3)"). A splitter, joiner, selector/slicer
or sorter is edited in a step window (its fields, summary and results,
and "apply" refusing a splitter with no separator), as is a string match
(its type, fixed text, character set or regex, its limits and example,
whether the example matches and why not, and "apply" refusing one it
fails), and a tag filter (its filter button opening the tag filter
editor, and whether its example passes: '"x" did not pass the tag
filter!'); a converter in the
string converter editor, which a sidecar's filename conversion button
opens too: its conversions numbered, each with the example converted up
to it (or the error), with add (starting from the last conversion made),
edit, delete ("Delete all selected?"), move up and move down, and the
example; each conversion in a conversion window (its type, the fields
the type has, and the example converted; "apply" asking first about a
regex that captures a group with no replacement). What they show at each
step is as `oracle/record_string_processor_editor.py`,
`oracle/record_string_match_editor.py` and
`oracle/record_string_converter_editor.py` recorded the reference's.
Date conversion fields now execute in downloader and sidecar consumers: advanced
strptime decoding honors UTC/local/offset, timestamp formatting honors the
reference's current local offset, and the easy parser accepts common ISO and
English dates and relative expressions. The conversion window and sequence rows
update their previews live, including invalid input. Typed date values retain
reference interchange codes 10/12/14; older native preserved date payloads also
execute. `oracle/dump_string_dates.py` and `oracle/record_string_date_editor.py`
record timezone/fraction/pre-epoch and Qt accept/cancel/reorder boundaries.
Conversion child acceptance saves the last-used conversion in the store, even
when its parent is later canceled. Add reads that shared preference after reopening
and falls back to preserved reference options.
`oracle/record_string_conversion_preference.py` records acceptance, child Cancel
and option serialization/reload across fresh parent editors.

The processor step list has import, export selected, and paste controls. Its
exchange window reads bounded reference JSON/PNG, reviews appended steps, copies
selected entries in execution order, and saves reference PNGs. Import updates
per-step/final previews in the draft; parent Cancel closes and invalidates the
exchange child. `oracle/record_processing_exchange.py` records real Qt single/
multiple selection exports, clipboard append, invalid input and a reference PNG.

Regex match and sorter controls offer saved favourite descriptions and copy the
chosen phrase to the clipboard, as the reference menu does. Manage favourites
opens a sorted phrase/description list with add, edit, extended selection and
confirmed delete. Invalid expressions are advisory and can be saved as fragments;
Add rejects an exact duplicate pair. Shared regex controls save accepted favourites
immediately, while the standalone editor returns a draft to its owner. Native
preferences override imported YAML, including an explicitly empty list.
`oracle/record_regex_favourites.py` records real Qt rows, duplicate and cancel
boundaries, advisory validity, defaults and menu copy behavior. The shared .*
control offers the reference component snippets; regex conversion
fields also offer pattern/replacement groups. Snippets copy to the clipboard
without changing the current input.

Help > about opens the about window (`ui/about.slint`,
`src/about_window.rs`, `hydrus-gui-model/src/about.rs`), as the
reference's "about hydrus": the name, version and site link over the
"Description" (platform, ffmpeg and SQLite versions, boot time,
directories and database settings), "Optional Libraries", "Credits" and
"License" tabs, in the reference's forms
(`oracle/record_about_window.py`).

File > options opens the options window (`hydrus-gui-model/src/options.rs`), as the
reference's "manage options" dialog: its pages listed on the left as
hydrus lists them (by name, "advanced" last), the page chosen on the
right, each option its label and then its control, in the page's titled
boxes, as the reference's dialog lays them out (checked against the
running reference's dialog, recorded by `oracle/record_options_dialog.py`).
It has the options hydrus-rs honours, so far on twenty-five pages: audio,
connection (retries, timeouts, job limits, the halt on a domain's errors,
HTTPS checks and proxies), downloading (gallery, subscription and watcher
waits, the default file limit, highlighting, the pause and stop
characters, short summaries' counts, the waits after errors, two
debug switches, and the default subscription and watcher checker
options), duplicates (the duplicate filter's batches and its
comparison score weights, and the duplicates page opened on files),
exporting, file search (the default tag service for new searches), file sort/collect (the default and secondary sorts, each a
page's sort types and then the type's orders, whether a sort chosen on
a page becomes the default, and the default collect, as a page's
collect control offers it), file viewing statistics (whether they
are kept), files and
trash, gui (keeping the media viewer's size and place), gui pages,
importing (looking inside .zip files for comics), maintenance and
processing (file maintenance in normal time and its throttle, the
potential duplicates search, auto-resolution in normal time and its work
and rest), media playback (the zoom centre, the zoom steps, the media
viewer's default zoom, and what counts as transparency), media viewer
(slideshows), media viewer hovers (the top hover's
file summary), ratings (the media viewer's rating sizes, and the
thumbnails': their sizes, which go up to the thumbnails' width as the
dialog opens, their box, and numerical ratings always collapsed), system (wake detection and its network grace period), tag editing (remembered or fixed tag-dialog service), tag presentation, tag sort (the search pages' and
the media viewer's default tag sorts: a type, its orders, and its
grouping where the type groups), thumbnails (their size and how
they fit it, their border and margin, the UI-scale supersampling, how far
into a video its thumbnail is taken, and the single file's text in the
status bar) and advanced. The grid is laid out as the reference's: each
cell is the thumbnail box and its border, with the margin all round, as
many across as fit (a click in a margin is on no file). Thumbnails given a new size show it at once: the grid's cells
take it, and a thumbnail made at the old size is shown scaled to the new
one and made again from its file, as the reference does. Times
show as the reference's fields (days, hours, minutes, seconds, ms), and a
rate as its number, the reference's words ("errors within") and a time;
text that may be none has the reference's "none" box. The window opens on
"gui", or the remembered last panel when enabled; navigation is remembered even
when edits are canceled. The gui page lets the search appear above or below the
pages on reopening. The gui page also edits the application name (including
the reference’s empty-name fallback) and exit confirmation. The main title uses
that name and the Rust version. Exit and the window close button ask the recorded
yes/no question when enabled, automatically accepting after 15 seconds; declining
keeps the client open. Its reference search box ("Search options... (Experimental!)"): as it
is typed in, it suggests box titles, option labels, auxiliary unit/none labels
and current dropdown text whose text has what
was typed in it (ignoring case), as "text (page)", ten at a time; the
arrows, enter or a click choose one, which shows its page with that row
highlighted, as the reference's does (and the page list brought round to
it). The daemon picks up the connection, downloading, maintenance and
thumbnail options within a second of "apply". Connection limits and the three
downloader error delays use the reference’s normal or advanced ranges when the
window opens (`oracle/record_options_ranges.py` records both modes and clamps).
The gui sessions page sets the number of rolling backups to keep (1–32);
the next session save uses that limit. The regex favourites page opens its
regex/description list editor; accepting that child stages the parent draft,
which Apply saves or Cancel forgets. Gui pages also chooses whether closing
the current tab focuses its left or right neighbour, and whether sending pages
to a new notebook prompts to rename it.
A "checker options"
button opens the checker options editor (`hydrus-gui-model/src/checker_options.rs`,
checked against the reference's `EditCheckerOptions`, recorded by
`oracle/record_checker_options.py`): its five reasonable defaults, the
file velocity below which checking stops, and static checking (its
check period) or reactive checking (intended new files per check, never
faster and never slower than); in advanced mode its times can go down to
a second. A time typed below never faster than moves never slower than up
to it, field by field as the reference's does, and a time below its
least is raised to it on "apply", which asks first if never faster and
never slower than are the same. Changes wait for "apply", which writes them
together (and shows again what they change, such as the tab names);
"cancel" or escape forgets them. A value that can't be had (slideshow
durations that aren't numbers) is left as it was and said in a popup, as
the reference says it.

A simple downloader page (pages > download > new simple downloader page,
or the page chooser; `ui/simple_downloader.slint`,
`hydrus-gui-model/src/simple_downloader.rs`) has the URL page's "imports"
box and the reference's "parsing" box: what its parsing is doing ("checking
...", 'page checked OK with formula "..." - 2 new urls', "paused") with
its own pause/play, the parsing log's status and button, the page being
downloaded, the pages waiting ("all images embedded in page: https://...")
with up, X and down, the URL box and paste button (full URLs only, each
once), and the formula chooser. Each page waiting is parsed by `hydrus
serve` with its formula, the files found downloaded as a URL page's are.
A new page starts on the formula last chosen. Its "edit formulae" button
opens the saved formula list: add, edit, remove (with confirmation), and add
defaults. Selected formulae edit in sequence; cancelling a child stops the
remaining edits. Each formula's name is entered before its reusable editor;
Apply saves the list and refreshes the chooser, while Cancel discards it.

The reusable formula editor (`formula_window.rs`,
`ui/formula_editors.slint`) edits ordered rules with add/edit/remove/up/down,
name, extraction mode and string processor. HTML rules search descendants or
previous/next siblings, or climb ancestors, with tag names, attributes,
optional indices and optional string matches. JSON rules select dictionary
keys, all items, an index, matching scalar values, ancestors or deminified
JSON. The test panel accepts a document and context variables, runs the real
parser with its caller's newline policy and shows results or the parse error.
The processor receives the parsed strings before processing. Formula controls
stay locked until an open rule or string editor closes. Processors and
converters require their child draft to finish before Apply; Cancel or
closing their window discards all unfinished descendants. JSON sidecar
sources open this same formula editor from "edit parsing formula", restricted
to JSON and preserving parsed newlines as the sidecar importer does.
Context-variable and static formula controls edit names, variable keys, text and
output counts (1–65,535), with the same parsing context, newline policy and
string processor preview as HTML/JSON. Type changes create fresh reference
defaults and reset the old name/processor. Edits remain isolated until Apply.
`oracle/record_recursive_formula_editors.py` records these scalar controls and
the recursive formula workflows. Nested formula editors open a staged first
formula and a second formula whose examples come from the first formula's parsed
strings. All transformed examples remain selectable. Zipper editors add/edit
component formulae recursively, reorder/remove them, import components and export
selected components, and edit the substitution phrase. Every depth supports all
six formula kinds. Parsed previews update when controls, documents, context,
children or processors change. Parent edits and Apply wait for open descendants;
owner cancellation invalidates every descendant callback. Saved page/content
parsers use these formula edits in the live parser engine. Reusable content editor
callers can restrict permitted content types while retaining staged Apply/Cancel
and arbitrary named parsing context for formula previews. Formula test panels fetch
URLs with live progress and cancellation. Page parser test panels fetch their page
URL with an optional referral URL, using the stored network options, approved
headers and cookies. Response charsets are decoded by the downloader engine;
failed page fetches retain the server document. Fetches preserve context variables
and reset the post index to zero. Fetched documents remain selectable alongside
pasted examples, and selected data reaches child parsers through the page converter.
Closing an owner cancels its request and discards late results. Bandwidth usage is
merged with other engines rather than replacing their traffic.
`oracle/record_parser_test_data.py` records these request and context semantics.

`oracle/record_formula_editors.py` records the real reference controls and
queue actions, checked by model and headless GUI/store tests.

A URL downloader page shows its queue as the daemon works on it, as the
reference's does: its sidebar's "imports" box has the file log's status as
hydrus words it ("2 successful (all already in db)"), its progress ("2/2")
and bar, and the reference's pause/play button, which pauses or resumes the
queue's files; the "search" box has its search log's status; under each,
the download in progress as the reference's network job control shows it
("downloading…", "472 KB/2 MB 56 KB/s" and a gauge), with a stop button
that cancels it (the file then ends ignored, "Download cancelled:
Cancelled by user."); and the files it imports, or finds already in the
database, join the page as they come, in the queue's order. URLs typed
into its box (enter) or pasted with its paste button, one per line, go to
the daemon, which adds those it recognises (as the reference's page does)
and starts on them within a second. A closed URL page's downloads wait
while Ctrl+U can bring the page back, and go after the hour or when the
client closes. Closing a page that holds anything, idle, asks only if
hydrus's option to do so is on (as it is by default).

A local import page (the reference's "import" page) shows the files it
imports from disk as the daemon works through them: its sidebar's
"imports" box has what the import is doing ("importing"), the
reference's pause/play button, and the file log's status, progress and
bar; its files join the page as they come, in order; and closing it asks
as the reference's does ("This page is still importing.", "This is a
local import page holding 3 import objects.").

File > import files… opens the reference's "review files to import"
window, and files dropped on the main window open it with them (dropped
while it is open, they join its list); "add files" and "add folder" ask
the system's file and folder pickers (the desktop portal on Linux), and a
path can be typed. It parses the paths given, folders
walked (their subfolders too, if "search subdirectories" is ticked) a
little at a time as the reference does, and lists them as the reference
does: #, path, filetype ("png", "PROBLEM: filetype unsupported", "file is
empty", "file is missing", "sidecar") and size, the problem rows in the
reference's error colour; under it, "6 files parsed - 4 good | 1 bad: 1
had unsupported file types - and looks like 1 txt/json/xml sidecars." and
a gauge, with pause/play and stop while it parses. "remove files" takes
the selected rows out (asking first), and "delete original files after
successful import" says, in the reference's red, that they will be
deleted. "import now" opens an "import" page of the good files, in the
list's order. Paths go in through the box over the list (enter adds one,
a file or folder, as typed or pasted with a file manager's quotes) in
place of the reference's "add files"/"add folder" pickers.

"add tags/urls with the import >>" opens the "filename tagging" dialog
(`src/filename_tagging_window.rs`, `hydrus-gui-model/src/filename_tagging.rs`),
the reference's `EditLocalImportFilenameTaggingPanel`: a tab per real
tag service, each listing the files (#, path, and the tags each gets,
shown again as the options change) over "simple" (tags for all, tags
just for the selected files, and the filename and directories as tags,
each with a namespace; typing a namespace ticks its box) and "advanced"
(quick namespaces, regexes, and a number for each file from a base by a
step, in a namespace). Its first tab, "sidecars", lists the files with
what each one's sidecars give (`to "my tags": creator:samus,
series:metroid`, "2 notes: ...", "1 URL: ...", "archived time: ..."), as
`oracle/dump_sidecar_previews.py` recorded the reference's, over the
"sidecar import routes" button, which opens the sidecar editors (see the
import folders below). "apply" imports the files with their tags, as
`oracle/record_filename_tagging.py` recorded the reference's dialog
giving them, and the import reads each file's sidecars as it goes in.

An import folder's dialog lists the tag services it tags files for by
their paths, with "edit" and "delete" for each and "add" for another
(refused, with the reference's warning, for one it has). "add" and
"edit" open that dialog's boxes on one service's options alone ("edit
filename tagging options"), with an example path from the folder and
the tags it would get.

A gallery downloader page is the reference's too. Its "gallery
downloader" box says how its searches stand ("2 queries - 4/6", and
"waiting for new queries" with none) and lists them as the reference's
list does: query, source, files and search status (the reference's pause
and stop characters), status ("DONE", "working", "pending"), items ("2 -
1Ign") and added ("5 minutes ago"), sortable by any column (clicked
again, the other way). Its rows are selected as the reference's lists
select them: a click selects one, ctrl+click adds or takes one away,
shift+click selects from the last clicked, and dragging across rows
selects from the row pressed to the one under the pointer (as every list
built on `ui/list_table.slint` does); the delete key removes them.
A double-click on a search, or its highlight button with one selected,
shows it (a double-click on the one shown stops showing it): the page then holds that
search's files, and the "highlighted query" box under the list has its
"imports" and "search" boxes, each with its live status line, its pause
button and its download in progress, as a URL page's do; the
clear-highlight button empties the page again. The list's buttons pause
or resume each selected search's files or its search, retry their ignored
or failed files, and remove them, asking first as the reference does
("Remove the 2 selected queries?", saying how many are still working and
that the page will be cleared if the one shown is among them). Each
search keeps the file limit and import options it was made with; while a
selected one's differ from the page's, "update selected with current
options" shows under the file limit, and gives them the page's, asking
first as the reference does. Queries typed into its box
(enter) or pasted with its paste button, one a line, become searches with
the downloader and file limit chosen under it; queries already on the
page are refused with the reference's message, and the first new one is
shown if hydrus's option to highlight new queries is on (as it is by
default). The downloader list offers the downloaders hydrus displays,
by name, then the rest; the page's own downloader and file limit are
kept as you change them, and carried over from hydrus.

A watcher downloader page is the reference's as well. Its "watchers" box
says how its watchers stand ("3 watchers - 12/40", "waiting for new
watchers") and lists them as the reference does: subject, files and
checking status (the stop character for a watcher whose thread died or
404'd), status ("pending", "working", when it next checks, "DEAD",
"404", and for fifteen seconds "just added"), items and added; by status
at first, and by any column. Its rows select as a gallery page's. Its
buttons highlight the one watcher selected (or a double-click does),
clear the highlight, pause or resume each selected watcher's files or
checking, check them now, retry their ignored or failed files, and remove
them, asking as the reference does ("Remove the 3 selected watchers?",
saying how many still work or aren't yet DEAD, and that the media panel
is cleared if the highlighted one is among them). While a selected
watcher's checker or import options differ from the page's, "update
selected with current options" shows under the page's "checker options"
button, and gives them the page's. All of this is as
`oracle/record_downloader_lists.py` recorded the reference's lists. Thread URLs typed into its
"watcher url" box, or pasted, one a line, become watchers with the page's
checker and import options; one the page watches already isn't watched
twice, and the first new one is shown if hydrus's option to highlight new
watchers is on. Its "checker options" button, under the box, opens the
checker options editor on the page's (the default watcher checker
options until changed), and what it applies the page's new watchers get;
those it has keep theirs, as the reference's do. The "highlighted watcher" box has its thread's subject
and URL, its "imports" (files line and pause, file log status and
progress, download in progress) and its "checker": how fast the thread
was getting files ("at last check, found 5 files in previous 1 day"),
checking's pause, when it checks next ("next check in 4 minutes",
"checking imminently") or what it is doing, a "check now" button, its
check log's status and its check in progress, and its own "checker
options" button: other checker options time its next check again (and
may find the thread dead, pausing its checking), the same change nothing
(`WatcherImport.SetCheckerOptions`, checked against the reference's,
recorded by `oracle/record_watcher_checker.py`).

On a search page, at the top of its sidebar, the
sort control offers every system sort type named and ordered as in the
reference, each with its two orders ("oldest first", "newest first"...),
then your namespace sorts ("tags: series-creator-title-volume-chapter-page",
a-z or z-a) and each rating service ("rating: stars", by name), and a
session's own namespace sort where a page has one; choosing a type picks
its default order, as the reference does. Pages sort
as the reference's pages do (`MediaList.Sort`, checked against it): your
fallback sort first (hydrus's default: import time, oldest first), then the
page's, so ties fall in the fallback's order, with files that have no value
placed as the reference's pages place them (never viewed as no views, the
inbox before the archive). A new page sorts by your default sort (hydrus's
own: file size, smallest first). A namespace sort orders files by their
subtags in each namespace in turn (current and pending, in all known tags,
in human order, as the reference's `GetComparableNamespaceSlice`); a rating
sort by the rating, unrated files counting as -1 (0 on an inc/dec
service). Under the sort control, the collect control ("no collections",
or "collect by series-stars") opens the reference's choices: the
namespaces in your namespace sorts, then the like/dislike and numerical
rating services, each checked to collect by it; its ⚙ says whether files
matching none collect into one group or stay separate. A page collects as
the reference's pages do (`MediaList.Collect`, checked against it through
the page): files group by their tags in those namespaces and their
ratings' values, every group a collection (even of one file), and the
files and collections sort together, a collection by the reference's view
of it (sizes summed, its latest import...). A collection shows as its
first file with its number of files in a box at the bottom left; the
status bar counts "N files in M collections"; selecting one selects its
files (the menu, the archive/delete filter, deleting... act on them);
activating one opens the media viewer over all the page's files from its
first. Collecting selects nothing first, as the reference does; sorting
leaves the collections as they were. A new page collects by your default
collect; a session's page as it was (collected and sorted on opening, as
the reference's page does); a page opened from the selection as its page
did. Not yet: the ⚙'s tag service (collecting goes by all known tags,
the reference's default), and middle-clicking the control to clear it.
With nothing typed, the search
box offers `system:everything`, `system:inbox` and `system:archive` with how
many files each finds in the page's file domains, counted as the reference's
`_GetFileSystemPredicates` does, then the system predicates that open an
editor ("system:dimensions", "system:time", "system:urls"...; searching all
known files, only those needing no file's metadata). Choosing one opens the
reference's "input predicate" dialog (`FleshOutPredicatePanel`;
`hydrus-gui-model/src/predicate_editors.rs` and `src/predicate_editor_window.rs`, checked
against the reference's own editors, `oracle/record_system_predicate_editors.py`):
a note over some, tabs for an editor of several pages ("system:time"'s
import, modified, last viewed and archived), the page's ready-made buttons
("system:ratio is square", "1080p", "system:limit is 256"...; file
properties two to a row), each adding its predicates, and its panels, a row
of fields each with an "ok" button, starting at the reference's defaults:
operators as drop-downs, numbers (a span of time in its units), "≈"'s amount
either side and "≈%"'s percentage shown when chosen, tick boxes, text
(greyed when it doesn't count, as the namespace is until "namespace" is
chosen), and what a panel can't make (a date that isn't one, a regex that
doesn't compile) said under them. What a panel makes is the reference's:
"number of tags" for a namespace with none or any becomes the namespace's
own predicate (`-character:*anything*`), an empty note name is "notes", an
invalid advanced tag is "invalid tag", and "≠" ratios are searched though
the reference's parser has no words for them. "system:filetype" has the
reference's tree of filetypes by group (a group's tick box ticking all of
it); "system:hash" reads hashes one to a line as the reference does
(a type and colon, or "0x", before each allowed), saying which lines
aren't hashes or that the hash type looks wrong, with its two clean-up
buttons; "system:rating" has a panel for each rating service (like or
dislike, stars, counts) under the all/any/only panel, whose services are
chosen in place (the reference's specifier button opens a dialog);
"system:similar files" takes file hashes, or pixel and perceptual hashes
with "Paste image!" taking a file's from a path on the clipboard. What an
editor adds, from a button, a panel or a recent one, is kept as a recent
predicate of its type, as the reference keeps them
(`hydrus_core::search::recent`, checked against
`oracle/record_recent_predicates.py`): the newest first, five of a type,
one added again moved to the front. Each page shows its types' recent
predicates over its buttons (the dimensions page its heights, widths,
ratios and numbers of pixels; "system:time"'s import page its import
times...), less any a button of the page adds, each adding itself again
or, with "forget", forgotten. Yours come across from hydrus. Not yet:
setting a panel's values as its default (the star).
Typing in the search box lists
the matching tags with their counts (display tags, in the page's file domains
and tag service; the exact match first, then the most used), up and down move the
highlight, and enter adds the highlighted tag, or the text as typed for a
system predicate; a leading hyphen excludes. Predicates are entered as the
reference's list takes them (`hydrus_search::enter_predicates`, checked
against `oracle/record_predicate_entry.py`): one already in the search is
taken out again; one that isn't comes in and takes out what it excludes
(system:everything goes as anything else comes, a predicate's inverse
goes, inbox for archive, a tag for its exclusion, "has audio" for "no
audio", one system:limit for another); and the list sorts itself as the
reference's does, by the text the reference would copy, in human order. So
it is from the search box, a tag double-clicked in the tag list, an
editor, and the favourite search dialog. Predicates are listed as the
reference writes them (`system:width>1920` shows as `system:width > 1,920`),
and double-clicking one removes it. Under the search box, as the
reference's autocomplete has them (`hydrus-gui-model/src/domains.rs`, checked against the
reference's buttons and menus, `oracle/record_search_domains.py`):
"include current tags" and "include pending tags", each switching to
exclude them from the search; the pause/play button ("searching
immediately", or "search paused", when a changed search waits to be
searched; ctrl+i in the search box switches it); and the file and tag
domain buttons. The file domain button says what the page searches ("my
files", "trash", "my files, trash", "3 services", "deleted files of my
files", "all known files with tags") and opens a menu of the domains to
search, the one searched ticked: each local file domain, all of them
together when there are several, the trash, "all files ever imported or
deleted", in advanced mode the repository updates, hydrus local file
storage, everything deleted and all known files (with tags), the file
repositories, and "multiple/deleted locations", ticked for any other mix,
which opens a list of tick boxes (in advanced mode, each domain's deleted
files too), ticking a domain that covers others unticking them, as the
reference's does. The tag domain button names the tag service searched
and opens a menu of the local tag services, the tag repositories and all
known tags. Choosing all known files while searching all known tags
moves the tags to the first local tag service, and choosing all known
tags while searching all known files moves the files to the options'
default local domain, as the reference's do; the page searches again (if
it searches as its search changes) and keeps its domains in the session. Below them, as the reference's
"selection tags" box, the tags of the selected files (or with nothing
selected, of every file on the page) with how many have each (`tag (3)
(+1)` for pending, `(-1)` petitioned), display tags in the page's tag
domain less those your tag display filters hide from it, sorted by your
search page tag sort and namespace order (checked against the reference's
`SortTags`); double-clicking a tag searches for it too. Tags here, in the
autocomplete and in the search's predicates are shown as your options
have them (namespaces shown or hidden, the namespace connector,
underscores and emojis replaced; checked against `RenderTag`), each in its
namespace's colour (system predicates in `system`'s; yours come across
from hydrus). The ★ beside
the search box opens hydrus's menu: "manage favourite searches", "save
this search", and your favourite searches in hydrus's folders and order
(a folder with "/" in its name nests; searches before folders, by name).
Choosing one loads its domains, tag service, predicates, sort and
collect into the page (the collect at once, on the files shown), searching
if it is synchronised. "manage favourite searches" opens hydrus's "edit
favourite searches" list (folder, name, search, sort, collect; sorted on
a header clicked, the folder by default, as hydrus sorts it), with "add",
"edit" (or a double-click) and "delete" (asking first), kept only on
"apply". "edit favourite search" edits folder/name, file/tag domains and
current/pending tags, search suggestions, sort type/direction, and collection
namespaces/ratings, tag domain and unmatched files. Blank system suggestions
open the shared predicate editors; accepting stages their predicates and
cancelling the favourite or manager cancels pending children. Sort/collect
save ticks keep these values optional. Collection uses the saved tag service
and both current and pending tags, as the reference does. Renaming onto
a search that exists asks before overwriting it. These controls and collection
results are recorded by `oracle/record_favourite_search_editor.py`. "save this search" opens
both on the page's search, sort and collect, named "new favourite search"
(with " (1)" and so on if any folder has that name). The lock button beside them
locks the page's search to a `system:hash` of the files in view (asking
first, as hydrus does, unless the search is empty or already that hash);
a page opened on files ("open in a new page") starts locked. A locked
page shows "search locked" and a "Locked at N files." button, which
unlocks it, in place of the search box and predicates; it doesn't search
on refresh, and files removed from it leave the hash, unless its cog
says otherwise. Locks come across from hydrus's sessions and are kept in
ours. The matching files' thumbnails fill the grid, newest import
first. The status bar says what the reference's does (`hydrus-gui-model/src/status.rs`,
checked against `_GetPrettyStatusForStatusBar`): how many files and of
what type ("14 jpegs", "27 images", "36 files"), their total size and,
if they all have one, duration; with files selected, the same of them
and how many are in the inbox, or for one file its interesting info
lines; and an empty page why ("no search", "no files found for this
search", a downloader page's "no highlighted query"). The grid is a list of rows, so only
the rows in view exist, and a thumbnail is read and decoded off the UI
thread when its row first comes into view (a blank frame until then).
Thumbnails are drawn as the reference draws them: each cell is your
thumbnail bounding box and its border (152x127 by default), and the
thumbnail sits in it at its own size (over your thumbnail DPR), centred,
never stretched. On a scaled screen it is resampled to the screen's
pixels first, so it is drawn pixel for pixel. Over it go the reference's
icons, where it draws them (`hydrus-gui-model/src/thumbnail_icons.rs`, checked against
the reference's grid painting the `basic` fixture's files,
`oracle/record_thumbnail_icons.py`): at the top right, from the right,
downloading, notes, the trash (or deleted from hydrus local file
storage) and the inbox; at the top left, sound (or play, for a file with
a duration and no audio) and the file repositories and IPFS a file is
in, pending to or petitioned from; and a collection's icon with its
number of files in a box at the bottom left (a collection showing what
any of its files would). The reference's tag banners go over it too
(`hydrus_core::tag_summary`, checked against the reference's
`TagSummaryGenerator`, `oracle/dump_tag_summaries.py`, and its grid's):
by default the creator, series and title across the top ("someone - metroid
- a test image"), and the volume, chapter and page at the bottom right
("v3-c10-p330-331", a run of numbers as its first and last), each in your
colours and namespaces from hydrus's options, from the tags a single file
shows (current and pending, in all tag services; a collection's from all
its files), clipped to the thumbnail as the reference's are. Over its
top right go the ratings of each rating service shown in thumbnails
(`hydrus-gui-model/src/thumbnail_ratings.rs`, checked against the reference's grid
painting the `basic` fixture rated in several ways,
`oracle/record_thumbnail_ratings.py`): only rated files', or every
file's if the service shows even unrated ones (an inc/dec count of 0 is
unrated). The like/dislike ratings share a row, each numerical rating
has its own, and the inc/dec ratings share the last, each row on a box
of the window's colour against the right border, and the icons at the
top right go under them. They are drawn as your options say
(`draw_thumbnail_rating_icon_size_px`, `thumbnail_rating_incdec_height_px`,
`draw_thumbnail_rating_background` and
`draw_thumbnail_numerical_ratings_collapsed_always`, which draws a
numerical rating as its "3/5" and one shape), with a numerical rating's
"3/5" on the left or right of its stars if its service says so, and
redrawn as soon as the options window's "apply" changes them; a
collection's are its first file's, as the reference's are. A rating set
in the media viewer, or a file archived there, shows on its thumbnail
at once.
Thumbnails are selected as in the reference's grid (v688's default one;
`hydrus-gui-model/src/selection.rs`, checked step by step against that grid driven in the
running reference, `oracle/record_thumbnail_selection.py`): a click
selects just the file (or, on one already selected, leaves the selection
be), ctrl+click adds or takes one away, shift+click selects from where
the last click started (a second shift+click moving the range's end), a
click between thumbnails selects none, and with the grid's keyboard ctrl+a
selects every file, escape none, the arrows, page up and down, home and
end move from the focused file (shift selecting as they go, from the file
last clicked; the grid scrolls to follow; with nothing focused, the file
focused last is selected again, or the next one if it has gone) and
enter opens the media viewer on the focused file; as the reference's media
shortcuts have it, ctrl+r takes the selected files off the page (not out
of the client; a selected collection goes whole) and ctrl+e opens the
focused file (not a collection) as the OS opens it, and ctrl+c copies the
selected files themselves. A right-click on a thumbnail selects it
as a click would and opens the reference's thumbnail menu
(`src/thumbnail_menu.rs`, checked against the reference's own menus for
several pages and selections, `oracle/record_thumbnail_menu.py`), so far
with the entries hydrus-rs can act on: first the selection's info (its
files' types and size, the focused file's info lines, and how often they
were viewed; a line chosen is copied to the clipboard, as in the
reference); refresh; select and remove (all,
inbox and archive, each file domain, local and not, not selected, none,
each with its count); rearrange (to start, back one, to here, forward
one, to end, as the selection allows; alt and home, left, right or end do
the same, and the order holds until the page sorts again); the
archive/delete filter; archive and re-inbox;
delete from each local file domain the selection is in (asking), delete
trash physically, delete physically and undelete; manage → tags and
notes (below); and urls → open in browser, open in a new page, or copy the focused file's
URLs (labelled by URL class, decoded as hydrus shows them, sorted), its
recognised URLs or all of them, and the selection's URLs of each class or
all of them (opening several asks first; "files with" a URL opens a "url
search" page on all my files); open → in a new page (locked to the files,
with the page's sort and collect), in a
new duplicate filter page (a duplicates page searching a `system:hash` of
them, on all my files unless hydrus's
`open_files_to_duplicate_filter_uses_all_my_files` was off), similar
files in a new page (for a focused still
image: a new page searching `system:similar to` the selected still images
at the reference's exact match, very similar, similar or speculative
distance, searching at once), or the focused file as the OS opens it (the
reference's default "Default OS File Launch"; per-type launch programs
aren't carried over yet) or in a web browser; and share → copying the
files themselves (as a file manager pastes them, as hydrus copies them),
the files' paths, hashes
(sha256, md5, sha1, sha512, blurhash, pixel hash; the focused file's shown
in the menu) and file ids. The single-file info menu's "show detailed
embedded file metadata" opens Detailed File Metadata from the thumbnails or
viewer. Its basics include every info line, with nested modified times indented;
local file decoding runs in a worker while the window shows loading. The EXIF
list selects and sorts normally, and double-click copies its raw value (plain
hex for bytes, including original NULs in text). XMP, IPTC, human-readable text
and extra info appear only when present; PNG EXIF includes the reference's
orientation note. PDFs show Author, Title, Subject and Keywords. Non-local files
show "This file is not local to this computer!" in human-readable text. Missing
local files leave basics visible and show a read error.
Not yet: clearing deletion records, manage's
duplicates, maintenance and viewing stats,
locations, urls → force metadata refetch, open's custom
similarity distance, and
share's copying of bitmaps.

Share → "export files" opens a manual export window for the selected local
thumbnails or the viewer's file (`ui/export_files.slint`,
`src/export_files_window.rs`, `hydrus-gui-model::export_files`). It previews
number, filetype and destination using the export folders' filename machinery,
adds ` (1)` suffixes for selected files whose names collide, remembers the
export phrase and destination, and removes selected rows after asking. The
existing sidecar routers editor supplies tags, notes, URLs and timestamps.
Copies overwrite existing destinations; links are optional. Export runs on a
worker with progress and cancellation between files. Trashing asks the
reference's confirmation and disables links; "export and close" asks "Export
as shown?" and closes after success. `oracle/record_export_files.py` records
the reference panel's previews, removal and confirmations, and its export
worker's collision filenames and overwritten file contents.
Export phrases accept either slash style between the final folder and filename
on Windows, as the reference does; native separators preserve nested folders.

Manage → "notes" (or "notes (2)", counting the focused file's notes; in
the viewer, the file shown's) opens the reference's "manage notes"
dialog (`ui/manage_notes.slint`, `src/manage_notes_window.rs`,
hydrus-gui-model's `notes_editor`, checked step by step against the
reference's, `oracle/record_manage_notes.py`): a tab per note by name (a
lone "notes" tab for a file with none), the note in view edited below.
"add" asks a name ("Enter the name for the note.", numbered "name (1)"
if a note has it), "edit current name" (or double-clicking a tab) asks
a new one, and "delete current note" asks first. "copy" copies every
note as the reference's JSON, "paste" merges JSON notes in as the
reference does (extending a note a pasted one extends, renaming on a
clash; text it can't read is said so), and "copy URLs" copies the URLs
in the note in view. "apply" writes the notes (each cleaned, empty ones
dropped) and deletes the ones gone; "cancel" with changes asks first.

Manage → "ratings" (there when there are rating services; the selected
files', in the viewer the file shown's) opens the reference's "manage
ratings for N files" dialog (`ui/manage_ratings.slint`,
`src/manage_ratings_window.rs`, hydrus-gui-model's `ratings_editor`,
checked step by step against the reference's,
`oracle/record_manage_ratings.py`): each like/dislike, numerical and
inc/dec service by name, its control drawn as the viewer draws it and
clicked as the reference's dialog controls are (left likes, sets the
stars clicked or counts one more; right dislikes, clears or counts one
less). A service the files differ on starts mixed, in its mixed
colours (an inc/dec one showing their average), and is left alone
unless set. "copy" copies the ratings not mixed as the reference's JSON
of service keys and ratings ("Copied 3 ratings!"), and "paste" sets
those it can take ("Pasted 2 ratings!"; text it can't read is said so).
"apply" writes the ratings changed to all the files.

Manage → "times" (the selected files'; in the viewer the file shown's)
opens the reference's "manage times" dialog (`ui/manage_times.slint`,
`src/manage_times_window.rs`, hydrus-gui-model's `times_editor`,
checked step by step against the reference's,
`oracle/record_manage_times.py`): the files' file modified, archived
and last viewed (media viewer, preview) times, each over all the files
("2 files set from ... to ..., 1 file without a time set"; an archived
or last viewed time no file has isn't there, and a file modified time no
file has can't be edited), each with its own copy and paste; the web
domain times (add, edit, delete; several selected are set together, and
editing one some files lack asks whether to give it to all the files)
and the file service times (imported, deleted, previously imported;
edit). Each time is edited in the reference's date-time editor
(`DateTimeEditorWindow`, hydrus-gui-model's `datetime_editor`,
`oracle/record_datetime_editor.py`): what the files had, the date, the
time to the millisecond, a cascading step when several files had the
time (each file in turn set that much later), now, copy and paste (a
timestamp, or a date string), never later than now. "copy" (one file)
copies all the times, or one kind, as the reference's serialised
timestamp data, and "paste" sets the times it names. "apply" writes the
times changed (asking first if more than 100 changes), a stepped time
file by file, and a changed file modified time to the files on disk too
(as the warning under it says).

Manage → "force filetype" (the selected files'; in the viewer the file
shown's) opens the reference's "force filetypes" dialog
(`ui/force_filetype.slint`, `src/force_filetype_window.rs`,
hydrus-gui-model's `force_filetype`, checked against the reference's,
`oracle/record_force_filetype.py`): its warning, what the files are and
are forced to ("Of the 4 files, there are 1 jpeg, 2 png, 1 webm. 2 are
currently being forced, to: 1 png, 1 mp4."), and the filetypes to force
them all to, in the reference's order ("remove all forced filetypes"
first when some are forced; a single filetype isn't offered itself).
"apply" forces them (a file forced to the type it was detected as isn't
forced) and renames each file on disk to its new extension; hydrus-rs
then shows and searches it as that type, as the reference does.

Urls → "manage" (the selected files'; in the viewer, the file shown's;
always the urls menu's first entry, as the reference has it) opens the
reference's "manage urls for N files" dialog (`ui/manage_urls.slint`,
`src/manage_urls_window.rs`, hydrus-gui-model's `urls_editor`, checked
step by step against the reference's, `oracle/record_manage_urls.py`):
the files' URLs, sorted, each counted ("https://a/1 (2)") and warned
about when there are several files. A URL typed in the box and entered
is normalised as the URL classes say and added to every file lacking
it; enter on an empty box applies. "paste" adds a URL a line; texts
that don't parse as URLs are asked about first. Delete removes the
selected URLs, a double-click moves a URL into the box to edit, and
"copy" copies the selected URLs, or all of them. "apply" adds and
deletes the URLs in the order they were changed, asking first if text
is left in the box.

Double-clicking a thumbnail opens the media viewer in its own
window on that file (fullscreen, as hydrus opens it by default, or as
your hydrus frame for it says; f switches between fullscreen and the
window it was, and its size and place are kept as it closes if you had
hydrus's option for that on), at its default zoom (fitted to the window, unless your
per-filetype zoom rules or your default zoom say otherwise): right and left
(or down and up, page down and up, or the mouse wheel) move through the
page's files, round from the last to the first as in the reference, home
and end go to the first and the last, and escape, enter, a middle click
or a double-click closes it; ctrl+r takes the file shown off the viewer and its page,
showing the next (closing with none left), and ctrl+e opens it as the OS
opens it, pausing one that plays; ctrl+c copies it, as a file. A
right-click opens the reference's viewer menu (checked against it,
`oracle/record_viewer_menu.py`): the file's info, as the thumbnails' menu
has it; zoom (in, out, to 100% or fit, to max, titled with the zoom);
go or exit fullscreen; the slideshow (below); volume (mute or unmute global and the media
viewer, force mute or unmute just here and stop forcing, for as long as
the viewer is open, and the volume, shown: a menu here can't hold the
reference's slider); remove from view; archive or return to inbox;
delete from each local file domain it is in (asking), delete physically
now and undelete for a file in the trash; manage → tags and notes; urls, open and
share, as the thumbnails' menu has them for the file alone; and the
player ("This is a MPV Embed Player."). Not yet in it: locations, and
manage's ratings, times, force filetype and viewing stats. The
slideshow submenu (`AppendSlideshowMenu`, checked against the reference's
as a slideshow starts, stops, resumes and shuffles,
`oracle/record_slideshow.py`) starts a slideshow at one of your options'
periods (1, 5, 10, 30 seconds and a minute, by default), very fast, or a
custom interval you type (one that doesn't read as seconds is said so);
stops it or resumes it at its period; and switches shuffling and playing
files through, for this viewer or (kept in the options) every new one. A
running slideshow shows the next file (or, shuffling, a random other one)
once its period has passed since the file was shown, by it or by you; a
file that plays bends the period as the reference's does
(`_CalculateAnySpecialSlideshowPeriodForCurrentMedia`, checked against
it for 2205 durations, periods and option sets): a short one moves on once
it fits a part of the period or some seconds, or stops at its end and
moves on if it is most of the period; one about as long, a little over,
stops at its end and moves on; one of no known duration stops the
slideshow; and with playing through on, a file that plays is shown until
it has played through. Unlike the reference's, a slideshow doesn't wait
while a menu is open (Slint doesn't say when one is), so the menu's
entries act on the file it was opened on, though the slideshow has moved
on (as does a deletion asked about); it does wait while the viewer asks
something (a deletion, a period). And mpv stops at a
file's end when told to (as the reference's code reads, its mpv forgets
being told: it clears that once it has loaded the file, which it does
after the slideshow tells it). Each file now starts playing though the last was
paused, as the reference's do. As the reference's default shortcuts have it, z switches between
100% and fitting the window, + and - (or ctrl and the mouse wheel) step
through your zoom levels and canvas fit, keeping the point under the
pointer still (or wherever your zoom centre option says), shift and the
arrow keys pan a twelfth of the way, and dragging moves the file; a file
zoomed wholly off the window is brought back, and resizing the window fits
it again. Video renders at the size it's shown, up to twice the window's.
With the pointer near the top of the window's middle three fifths, the
reference's top hover frame shows there: the file's place in the list
("3/30"); buttons, with the reference's icons, to archive it or return it
to the inbox, to send it to the trash (or, from the trash, delete it
completely), and to undelete it (one deleted from any of your local file
domains), each shown as the reference shows it; the zoom ("266.67%") and
buttons to zoom in, out and switch, to switch fullscreen, to open the file
externally, to show it, selected, in your file browser (in the place of
the reference's button to drag the file out to other programs, which
Slint can't do), and to close the viewer. Under them, the file's info line,
as the reference's has it: its "interesting" info lines
(`hydrus-gui-model/src/info_lines.rs`, `GetPrettyMediaResultInfoLines`: size, type,
resolution, duration and frames, audio; imported, deleted or in the trash;
modified, if far from its import; archived) joined with ` | `, as your
file info line options say (now migrated); archiving or returning the
file to the inbox, or deleting it, updates it. Its notes show on the right, under the
ratings, each name bold over its text, while the pointer is over them, as
the reference's notes hover frame does. With the pointer near the window's top right, the file's ratings show
there, as in the reference's top-right hover frame: each like/dislike
service, then each numerical, then each inc/dec, in its service's shape
(a named SVG is drawn as the fat star) and colours, at your viewer rating
sizes. A left click likes (again, unrates), a right click dislikes; on
stars, a left click (or drag) sets the stars there and a right click
unrates; on an inc/dec, left adds one and right takes one away.
Under a file mpv plays, the reference's scanbar shows how far through it
is (by frame for an animation of several, with `13/240 - 0.480/9.600` as
text), a thin line until the pointer comes near the file's bottom, then
full height; a click or drag on it seeks there, and ctrl and left or right
seek back 2.5 seconds or on 5 (past the end, round to the start), and
ctrl+b and ctrl+n go a frame back or on (mpv's frame steps, which pause),
as the reference's defaults have it. Ugoiras and animated WebP, which the client
plays itself, have the scanbar too, by frame: a click or drag goes to the
frame under the pointer, and ctrl and the arrows go to the frame showing
that much earlier or later (or if that is the frame shown, the one
before or after it), and ctrl+b and ctrl+n a frame back or on, round
the ends, timed as the reference times them (a WebP's frames
by its frame chunks). Playing pauses while the scanbar is dragged and
resumes when it is let go, as in the reference. A file mpv plays that has
sound plays at your hydrus volume (70 by default; the media viewer's own
if you had it use its own), muted if hydrus's global or media viewer mute
is on, and its scanbar has the reference's volume control at its right:
a click there (or ctrl+g, in the viewer or the main window) mutes and
unmutes everything, and with the pointer on it the volume slider and the
viewer's own mute show above it, the scanbar staying full meanwhile;
changes are kept, as hydrus keeps them in its options.
Slint's software renderer scales images by their nearest pixel, so stills
are also drawn as the reference draws them: the part showing is cut out
and resized with the file type's zoom qualities (area shrinking, Lanczos
growing, by default; cubic is drawn with Lanczos), off the UI thread, to
exactly the pixels it covers, over the quickly scaled still once ready
(`src/still.rs`; the filters do the same). F3 manages the
file's tags, as the reference's dialog does on the local tag services:
the file's tags on the service chosen, and an input whose tag, entered,
is added to the file (or removed, if it has it already), the tag as typed
offered first and then the service's tags matching it; double-clicking a
listed tag removes it, and changes wait until enter with nothing typed
(or "apply") writes them, escape forgetting them. The dialog opens on the
configured default tag service, falling back to its first local service if the
saved service is unavailable. When remembering is enabled, switching service
updates that preference immediately; it survives cancelling tag drafts and is
used by the next dialog. `oracle/record_tag_dialog_defaults.py` records these
option interlocks and real manage-tags tab changes. As the reference's
default media shortcuts have it, F7 archives the file, shift+F7 returns it
to the inbox, delete deletes it (from the page's domain, if it searches
one, else to the trash; in the trash, for good, unless the delete lock
holds it), asking first, and shift+delete undeletes it; a file deleted out
of the page's domains leaves the page and the viewer (which closes when
none are left). The same keys (and F3) work on the thumbnails, once a click gives
them the keyboard, for every file selected: F7 and shift+F7 on several
ask first ("Archive 3 files?"), as the reference's defaults have it, and
manage tags says how many files it has ("manage tags for 3 files"). F12 opens the archive/delete filter, as the reference's
does, on the files selected (or all the page's files), those in a local
domain and not in the trash: a left click or F7 keeps the file, a right
click or delete deletes it, a middle click or backspace goes back, up
skips it, and F12 or escape stops; finishing (or stopping, with anything
decided) asks, as the reference's "filtering done?" does, to keep N and
delete M from the page's domain: enter commits (archiving the kept,
deleting the deleted, with the delete lock's "inbox deletees" option
honoured), f forgets, escape goes back to filtering. The deleted leave
the page. Files zoom and pan there as in the media viewer (but for
dragging, as a click decides), and enter stops, as the reference's
media viewer shortcuts have it. With the pointer
over the window's left fifth, the file's tags show there, as the
reference's tags hover frame does: its display tags less those your single
media filters hide, in your media viewer tag sort and namespace colours,
pending ones marked `(+)` (it doesn't scroll yet); near its top, the
file's info line, as in the media viewer. Images are shown
whole. Ugoiras and animated WebP play with the client's own player, as
the reference's defaults have it: frames are decoded on a thread of their
own a few ahead of the one shown, each shown for its duration (a ugoira's
from its animation.json, else its timing notes), looping, and space
pauses them (`src/animation.rs`; animated JPEG XL shows its first frame).
Video, audio and other animations play in mpv: libmpv is loaded when first
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
quality, alternates, not related, a custom action, skip, go back. A
custom action asks which decision, then (for "this is better", "same
quality", or "alternates" in advanced mode) its merge options in the
merge options editor, for that decision alone, then which files to
delete ("delete neither", "delete this one", "delete the other", "delete
both" or "forget it"), as the reference's does. As in the reference, a
left click on the file is "better, delete the other", a right click
"alternates", a middle click goes back and up skips; video, audio and
animations play as in the media viewer, and the next three pairs' files
are decoded ahead. Files zoom and pan as in the media viewer (but for
dragging), and switching to the pair's other file keeps the zoom and
position, as the reference's does: the other file as tall as the first
was if both are landscape, as wide if both are portrait (otherwise by
whichever side differs less), unless at the default zoom that would
spill a little over the window's edge; a new pair starts fitted and
centred. Pairs come a batch
at a time (`duplicate_filter_max_batch_size`), or a group at a time in
group mode; pairs with a file already merged away or deleted are skipped;
decisions wait for the batch's end, which asks to commit them (unless
they are few, as `duplicate_filter_auto_commit_batch_size` has it), and
closing with decisions pending asks too. Merges use your duplicate merge
options. The model is `hydrus-gui-model/src/duplicate_filter.rs`, tested in
`tests/gui/duplicate_filter.rs` with the window drawn headless.
The duplicates page's sidebar has the reference's three tabs
(`ui/duplicates_page.slint`, `src/duplicates_sidebar.rs`,
`hydrus-gui-model/src/duplicates_page.rs`). Preparation says how many
files are eligible for the similar files search and how many it has
searched at the search distance (named by the distance menu, "exact
match" to "speculative", or set exactly), names the tab "preparation (60%
done)" while there is work left, starts and stops working hard, and from
its cog menu switches the search on in idle and normal time and deletes
every potential pair to search again (asking first). Filtering launches
the duplicate filter, and its "edit default duplicate metadata merge
options" menu edits the client's merge options for "this is better",
"same quality" and, in advanced mode, "alternates", in the reference's
editor (`ui/merge_options.slint`, `src/merge_options_window.rs`,
`hydrus-gui-model/src/merge_options_editor.rs`): the tag services whose
tags move or copy (each through a tag filter, edited in the tag filter
editor) and the rating services whose ratings do, added by service and
then action ("this is better" alone asks one), edited and deleted (asking
first); whether archived status, file modified times, known urls and
notes sync; and the note merge settings. What it says and asks at each
step is as `oracle/record_merge_options_editor.py` recorded the
reference's. Auto-resolution lists the rules with their progress
("5 to search, 2 still to test, 3 pairs resolved") and status, pauses and
plays the selected, switches auto-resolution on in idle and normal time,
and resets the selected (or every) rule's search, tests or denials,
asking first. What the tabs say is as
`oracle/record_duplicates_preparation.py` and
`oracle/record_auto_resolution_rows.py` recorded the reference's.

"edit rules" opens the reference's "edit rules" dialog
(`src/auto_resolution_rules_window.rs`,
`hydrus-gui-model/src/auto_resolution_rules.rs`): the rules with their
search, comparison, action, progress and operation, its warning, and
"add suggested" (the reference's suggested rules), "add", "edit" and
"delete" (asking first); "apply" writes them (a changed rule starting its
work over, as the reference's does). The rule editor sets the name,
paused, operation and most pending pairs, and has tabs for the search
(which pairs, the location both searches search, chosen in the
"multiple/deleted locations" list, the searches typed a term a line, the
distance and pixel
duplicates), the comparison (the comparators, added by kind and edited
in their own editors: a search for A, B or either; A or B's progressive
jpeg test; a relative test of a file property with its operator,
multiplier and delta; a pair test; visual duplicates; and OR and AND
lists of comparators, each edited in an editor of its own) and the
action (which, deleting A or B, and default or custom merge options,
starting from the client's for the action and edited in the merge options
editor).
"apply" refuses a "better" rule whose comparators can't tell A from B,
with the reference's words. Its "preview" tab searches the rule as
edited for a sample of its pairs ("only sample this many", or all) and
tests each, off the UI thread: "47 pairs searched; 10 matched", the
pairs that will be actioned (A and B, which way round, what the rule does
and what it would change) and those that will be skipped, either list's
pairs opening in the duplicate filter from the one double-clicked; a changed
search fetches again, changed comparators or actions test again, and a
rule that can't be had says why. What they say is as
`oracle/record_auto_resolution_summaries.py` recorded the reference's.

"review actions" (or double-clicking a rule) opens the reference's
"review duplicate auto-resolution actions" window for each selected rule
(`ui/auto_resolution_review.slint`, `src/auto_resolution_review_window.rs`,
`hydrus-gui-model/src/auto_resolution_review.rs`): the rule's name over
three tabs, opening on "pending actions" for a semi-automatic rule and
"actions taken" otherwise. Each lists pairs with both files' thumbnails
("Found 2 pairs."), sampled to a number or all, and fetched again by
"refresh", or when shown if it had nothing or changed. "pending actions"
lists what the rule would do to each pair waiting for you (its action,
then each file's changes by service, "my tags: add tag mappings: red |
delete tag mappings: blue", as `oracle/record_merge_summaries.py`
recorded the reference's summaries), and "approve"
and "deny" act on the selected (asking first for more than five),
leaving the earliest row's successor selected; "select all" selects them
all. Double-clicking a pending pair opens the duplicate filter on the
pending pairs from it, with "approve" and "deny" (the rule's action on
the pair as listed, whichever file is shown) over the usual decisions;
closing it after committing fetches every tab again. Double-clicking an
actioned or denied pair opens the media viewer on its files still
stored, and right-clicking selected rows offers "show selected row in a
new page" (or "show 3 rows in a new page"), their files deleted or not. A fully automatic
rule's tab says it won't wait for approval.
"actions taken" lists what was done and when, newest first; "undo"
(asking first, with the reference's warning) undeletes the files and
dissolves their duplicate groups, so they are searched again. "actions
denied" lists the pairs you denied and when; "undo" queues them to be
searched again. "edit rules" closes these windows, as the reference's
does. What the tabs list is as `oracle/record_auto_resolution_review.py`
recorded the reference's, step by step.
`crates/hydrus-gui/tests/gui/search_page.rs` drives the page and
`tests/gui/session.rs` a saved session, and both draw the window headless (the
screenshots land in `target/tmp/`). Not yet: the reference's menu of
closed pages (Ctrl+U reopens them one at a time), managing tags on tag
repositories (pending and petitioning), dragging thumbnails, the rest of
the thumbnails' menu, system
predicates in the autocomplete, the rest of the viewer's hover frames
(the top one's zoom options, volume, shortcuts and view options menus,
window move and embedded metadata buttons, and its tooltips; editing,
copying and hiding notes), the volume shortcuts
other than the global mute, the scanbar's buffering
shading, playing animated JPEG XL, a
download's cog and error menus (bandwidth rules, the last error); in
the duplicates page, editing its search and quick and dirty processing;
in the duplicate filter, deleting from the filter and the hover frames;
the viewer's other zoom shortcuts (fill, max, the zoom menu), its
zoom and pan locks, and "open externally" for the file types shown with
that button (their thumbnail fills the window instead).

### Downloader definitions

Network > downloaders > url classes opens the native class list. Add, edit,
duplicate and delete are staged until Apply. The list sorts and supports
Ctrl/Shift selection; its URL test selects the matching class in the current
draft. Class editors expose domain and regex masks, subdomain handling, ordered
path component matches/defaults, query parameter matches/defaults/ephemeral
values and their string processors, single-value matches, header overrides,
normalization flags, API and referral converters, and gallery page indices.
The example shows matching, stored/request URLs, API/referral URLs and the next
page. Invalid examples and defaults keep the editor open. Applying refreshes
URL matching immediately; cancelling leaves the native settings unchanged.
Definition drafts stay modal while their rule or string editors are open:
the parent fields and editing controls are disabled until the child closes.
Closing a child restores the parent controls and reloads their draft values.
Cancelling a converter or default processor also closes its nested editors,
so the definition can be closed or edited again without abandoned windows.

Network > downloaders > gallery url generators has single and nested lists,
with add/edit/duplicate/delete, template and replacement controls, search term
separator, initial/example searches, raw/request URL previews and matched
classes. Nested generators select existing single generators; missing members
are reported in the list and repaired on Apply, as in the reference. Deleting
a generator used by a nested one asks before removing it. Parser and URL link
settings are preserved when saving these lists.

Recorded by `oracle/record_downloader_definitions.py`; replayed by model and
GUI `downloader_definitions` tests, including real native-store reopen and
cancel checks.

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
   renders it. The work itself (downloads, subscriptions, folders,
   maintenance) runs in the daemon, `hydrus serve`, which the GUI starts
   when none is running; the GUI only shows and controls it (DECISIONS.md,
   2026-10-01).
2. **View models in plain Rust, views in Slint.** Each screen's behaviour
   (what a click or a key does, what is selected, what is fetched) lives in
   a testable Rust type; Slint files only lay out and bind. Behaviour is
   tested without rendering; layout is checked with screenshots from Slint's
   software renderer, which runs headless. What needs no window (and no
   Slint type) lives in `hydrus-gui-model`, tested in its own
   `tests/model/`; a change there rebuilds and tests in a couple of
   seconds, where the windows' crate takes half a minute. The windows'
   tests are `hydrus-gui`'s `tests/gui/`, one test binary (`cargo test -p
   hydrus-gui --test gui <part>::`). A test that needs no window goes in
   the model's.
3. **Order, by the owner's use**: a search page (autocomplete, thumbnail
   grid, sort and collect) and the media viewer; then downloader pages
   (gallery, URL, watcher); then the duplicate filter, built to commit
   decisions as they are made and to fetch pairs without re-reading the
   whole search space (all done in a first form). From here (DECISIONS.md,
   2026-10-02): the main window's frame and menu bar; popup messages; the
   options dialog; every page openable from the menus; the downloader
   pages' remaining parts; importing files; the search page's daily-use
   gaps. Then the rest: the duplicates page's other tabs, metadata
   editors, the dialogs for subscriptions and folders, shortcuts and the
   downloader editors.
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

Services → review opens the service registry with each local and built-in service's name, type, database id, service key copying and refresh. File-domain sizes and deleted counts, tag mapping/tag/file counts, and rated-file counts come from a consistent native store read and match `oracle/record_services.py`. Refresh retains the selected key. Bulk maintenance and remote administration actions show an explicit unavailable explanation.

Services → edit opens the manage services list (`services_editor`, `services_editor_window`, `services_editor.slint`). Add local file, local tag, like/dislike, numerical and inc/dec rating services; edit their names and rating display colours, shapes/SVG names, thumbnail flags, stars, zero, padding and fraction placement. Names acquire casefolded duplicate suffixes. Child editors and the list hold changes until their apply buttons; either cancel forgets its changes. Deletion asks before staging and again before Apply, rejects nonempty local file domains and the last local file/tag domain, and rechecks inside the atomic write. Apply republishes the registry/graphs, reconciles deleted-domain membership, rebuilds tag counts and invalidates visible thumbnails. It refreshes the current selection's tags even on a locked page and refreshes an open viewer's tags when deletion changes sibling or parent display. Concurrent service changes reject the stale editor without partial writes.

## Tag siblings and parents

Tags > siblings and tags > parents open service editors on local tag services
and tag repositories. Enter tags on each side, then add (parents can add every
child/parent combination); double-click a preview tag to remove it. The
relationship table sorts by its columns and supports Ctrl/Shift selection,
Delete and double-click removal. Conflicting siblings and links that would
close cycles are removed automatically, with the reference's replacement
reasons. Changes stay staged as (+) and (-) rows until Apply; Cancel drops all
service pages' changes. Removing a pending or petitioned pair asks whether to
rescind it. Apply asks about a complete uncommitted input pair. Show all pairs,
show pending and petitioned groups, the parents' show whole chains, and wipe
workspace filter the remembered groups. Clipboard/.txt import and selected
pair export use alternating tag lines. Repository changes ask for reasons and
remain pending/petitioned through the existing store content status machinery.
Applying updates the display graph and autocomplete counts atomically before
the main page refreshes its tags. `oracle/record_tag_relationships.py` records
the reference panels' labels and local/remote action-context transitions;
model replay, snapshot/count rollback checks, and real-store menu/window tests
cover the implementation.

**Tag display/search** (`tags > display/search`) edits each tag service's single
file and selection display filters with the native tag-filter editor, plus
fetch-as-you-type, character threshold, query rules and write autocomplete
file/tag domains. Domain changes use the native location selector, including all
known files and existing combined domains outside advanced mode. Ctrl+Space
fetches search/manage-tags suggestions manually. The advanced `manage where tag
siblings and parents apply` window edits ordered source queues in ListTable;
empty queues disable that relationship kind. Both dialogs stage changes until
Apply, disable parent editing while a nested editor is open, close owned nested
editors on cancel and preserve unrelated settings.
Applying updates selection tags on all open pages (including locked pages),
viewer tags, search suggestions (including an open manage-tags draft), graphs
and counts. The real reference panels
and checkbox interlocks are recorded in `oracle/record_tag_display.py`; pure model
and real-store GUI regressions cover persistence and publication.

The parser editor foundation (`hydrus-gui-model::parser_editors`) owns native
page/content drafts and direct URL-class links. Its nine typed content kinds,
URL/post-index/context-variable tests and real parser previews replay
`oracle/record_parser_editors.py`, which drives the reference's content,
page, named parser and URL-class link panels. Persistence updates parser keys
and links in one transaction, preserves generators and unrelated URL settings,
and rejects concurrent parser/link edits. The existing downloader reloads
changed parser definitions through its normal settings refresh; a local site
regression proves an existing downloader follows an edited parser and link.
Network > downloaders > parsers now opens the native named parser list
(add/edit/duplicate/delete), and url class links opens staged direct parser
associations. Page editors edit names, example URLs, pre-parsing converters and
content nodes. Existing subsidiary rows expose separation-formula editing and
source-time sorting. Their formula editors receive the parent's converted test
document and preserve parsed newlines. Separation edits remain staged in the page
draft and preserve each subsidiary's child parser. Content editors support URLs, tags, notes, hashes, timestamps,
titles, headers, temporary variables and vetoes, reusing the six-kind formula
and string-match editors. Test panels accept the document, page URL, post
index and validated key=value context variables, and run the live parser engine.
Parser and formula test panels keep multiple editable examples, with add/remove
and selection controls. Switching documents restores their source URL. Page
children receive every converted example with the selected document first;
formula children preserve all inherited texts and sources. Child editors block
parent changes and Apply; cancellation or owner closure invalidates all child
callbacks. GUI/store regressions include rendered page
and note-content screenshots through HYDRUS_PARSER_SCREENSHOTS.

Parser deletion confirms a snapshot of stable parser keys; selection, sorting
and other list actions wait until Yes/No. Applying changed parser associations
checks the current URL class's type and redirect converter in the transaction,
so a concurrent class edit cannot install a link on a file or redirect source.

Service review now opens a native Client API access-key list with the reference
columns, extended selection, sorting, add/edit/duplicate/delete, copy-key and
local base-URL opening. Permission editors expose all 14 basic permissions,
full access, the reusable permitted-search-tags filter and explicit key rotation
with validation/collision refusal. List and nested edits remain detached until
Apply; Cancel and closing service review cancel their descendants. Real Qt
controls/questions are recorded in `client_api_admin.json`; actual native-store
GUI regressions render `client_api_keys.png`, `client_api_permissions.png` and
`client_api_service.png` during the GUI test batch.

Manage services narrowly permits editing the built-in API service's enabled
state, port, local/network binding, CORS and anonymous request logging. Other
imported flags remain preserved and display plain values, with unset external
URL fields labeled "not set". Imported HTTPS can be disabled; enabling it
is unsupported. The daemon notices configuration changes within one second and
restarts only the API listener, keeping downloads, queues and authentication
sessions alive. A bind failure reports its cause and recovers after settings
are corrected; HTTPS reports a failure instead of silently serving HTTP.
Explicit CLI `--port` and `--bind` overrides retain precedence.
The local daemon regression additionally mints an authenticated session before
listener reconfiguration and uses that same session after bind-failure recovery.

Opening the API base URL uses the daemon's reported listening address, so CLI
port and binding overrides are honored even when the saved service is off.
Wildcard binds open through loopback; IPv6 URLs retain their brackets.

## Clipboard URL monitoring

Network > downloaders > watch clipboard for urls has independent persistent
switches for watcher URLs and other recognised URLs. The desktop checks changed
clipboard text once a second, ignores unknown URLs and recognised URLs without
a required parser, and sends accepted URLs to the current compatible importer
or the first open one. A new importer is created when needed, leaving the page
already selected in view; an empty notebook selects its first new child.
Unchanged text is routed only once, and switching either option lets the current
clipboard be examined again. An access failure produces a global popup and
suspends reads until a switch is toggled. Imported reference options carry both
switches over. The real Qt watcher and URL-routing policy are recorded by
`oracle/record_clipboard_urls.py`; native tests cover page routing, nested
notebooks, menu persistence and failure recovery.

## Network sessions and HTTP headers

Network > data > review session cookies browses persisted domain and imported
service sessions, with a text filter, show-empty toggle, cookie count and latest
expiry. Create new establishes a domain silo; review opens its cookie list.
Clear asks the reference's deletion question and removes the selected sessions.
The cookie window stages add/edit/delete until Apply, including domain/path/name
changes, session or UTC expiry, a time delta from now, and HTTPS-only cookies.
Other attributes, including HttpOnly and SameSite, survive an edit. Cancel closes
child editors and discards the draft. Empty sessions persist across reopen.

Both the browser and cookie list export selected cookies as the reference's
five-field JSON and import clipboard JSON through a separate confirmation window.
A session import offers matching-domain cookies, everything, or cancellation when
other domains appear. Matching identities are replaced; cookie-list imports remain
staged until Apply, while browser imports save directly into the domain silos.
The cookies.txt picker accepts Netscape/Mozilla files and preserves domain/path,
HTTPS-only and HttpOnly attributes, including session and expired cookies. It
reports unreadable or malformed files without importing a partial batch.
`oracle/record_cookie_exchange.py` records exports, choices, confirmation text,
replacement semantics and Netscape fields; widget tests exercise these controls,
cancellation and errors. An existing engine test sends imported cookies on its
next request and suppresses expired, wrong-path and HTTPS-only cookies on HTTP.

Network > data > manage http headers stages global and domain header names,
values, approval and reasons, with filter, sorting, duplicate and confirmed
delete. When a live daemon request waits for pending headers, the desktop opens
one automatic question per header, with its context, value and reason, even when
network review windows are closed. Yes approves and No denies that exact header;
changed or removed values cannot inherit a stale answer. Requests sharing the
header share its decision. Later leaves it pending and suppresses that exact
question until the request set or header changes. Approved values affect the next
request from an already-running engine, while denied values are omitted.
`oracle/record_header_approval.py` records the actual validation process questions
and stored yes/no decisions; native tests cover dialogs, stale callbacks and an
existing engine request resumed with the approved header only. Cookie and header
Apply merges only edited keys and rejects a concurrent change to those keys
atomically, preserving unrelated cookies received from websites or API writes.

`oracle/record_network_sessions.py` records real Qt session/cookie/header rows,
trim/newline validation and cancelled delete/clear questions. Model and native
widget tests cover cancellation, persistence, stale editor callbacks, concurrent
writes and actual outgoing cookie/header values through an existing NetEngine.

## Bandwidth and current network jobs

Network > data opens native bandwidth usage/rule and current network-job reviews.
Bandwidth review filters contexts by recent request usage, with custom seconds,
show-all, and an option to include anything with specific rules. The last age or
show-all choice persists when reopened. Rows show current speed, day/history/month
usage, specific-rule ownership and blocked time. Selecting one context shows its
all-time total, usage against each rule and a scrollable monthly byte-history bar
chart with UTC month labels; unused contexts show the reference's empty-history
message. Extended selection supports confirmed deletion of all selected history.
Deletion retains their rules, clears durable/live usage and reaches the running
engine on its next heartbeat, waking bandwidth waiters. Old engine saves cannot
restore deleted counts. The real Qt filter, chart totals and deletion paths are
recorded by `oracle/record_bandwidth_history.py`. Detached rule editors add, replace
and delete data/request limits with rolling-second or calendar-month periods.
Apply preserves unrelated context edits and pacing settings; Cancel leaves them
untouched. Default/global rules can be edited by kind or reset with the reference
confirmation, and specific domains can inherit their defaults again.

Current jobs show every active engine request, including subscriptions, with
URL, status, typed wait reason, speed and progress. Extended selection supports
cancel and bandwidth override, with selected-job context details and manual or
live refresh. The daemon publishes usage and jobs through typed local store IPC,
independent of the Client API listener. Heartbeats expire after five seconds;
offline bandwidth review falls back to saved history. Commands identify the
reviewed daemon and request, so finished jobs or replacement daemons ignore them.
The real Qt controls/rows/questions are recorded in `network_data.json`.
Backend/model regressions cover cancellation, dropped futures, live overrides,
settings reload, stale heartbeat, concurrent edits and persistence. GUI regressions
cover draft cancellation, owner closure, invalid values, Apply/reopen and local
commands, and render the review, rules and current-jobs windows.


## Service-to-service tag migration

Tags > migrate, service review's local/repository tag pages and Manage Tags' selected files open
"migrate tags…". The global entry uses the configured or remembered default
tag-dialog service and has no selected-file restriction. Choose mappings,
siblings or parents; a real source and
destination; current/deleted source content (also pending or current and pending
for repositories); and the actions available for the destination. Local services
support add/delete, plus clear deletion records for mappings. Repository actions
pend/petition local proposals, with an editable petition reason. Mapping scopes
use selected files or the existing multiple/current/deleted file-domain selector.
Mappings and each side of pairs use the reusable tag-filter editor.

The window shows the reference summary and its second confirmation outside
advanced mode. Migration runs on a worker in bounded atomic batches, with live
progress and cancellation. Cancelling retains committed batches; closing a
running job requests cancellation and waits for its final committed progress.
Services are resolved by key again on every batch. Graph/count publication occurs atomically per batch; displayed tags and review
counts refresh after completion or cancellation. Reference controls, questions and actual DB mapping
and pair destinations are recorded in `oracle/fixtures/tag_migration.json`.

## Downloader definition interchange

Network > downloaders > import/export downloaders exchanges URL classes,
page parsers and single/nested gallery URL generators with the reference
client's clipboard JSON and real downloader PNGs. Imports review the concrete
objects and exact duplicates before saving all definitions, generated keys,
nested members and parser links atomically. Exact parser duplicates merge
example URLs; imported parser examples link the appropriate URL classes.
Concurrent changes reject a stale import without partial writes. Native
URL/GUG/parser lists, page/content editors, reusable formula editors and the
simple downloader formula list also expose import/export; their imports remain
in their owner's draft until Apply. Cancel drops pending imports and invalidates
closed child callbacks. All six formula kinds and subsidiary parsers retain
reference editor data through native edits and export. The exchange codec is
separate from the read-only legacy reader. `record_downloader_interchange.py`
checks real reference PNG/JSON -> native encoders -> real reference loads;
codec/model/GUI regressions cover bounds, unsupported data, cancellation,
duplicates, stale snapshots and live downloader settings reload.

Tab right-click offers the reference's four move-page destinations and six
sibling sorts (file count, total size and name, both ways). Actions target the
clicked notebook row and preserve its selected leaf, including nested notebooks.
File-count ties include importer total/completed progress; name ties use file
count descending and exact lexical names. Equal keys remain stable. Recorded
against actual reference methods on Qt tabs by `oracle/record_tab_context.py`;
model replay and session GUI tests cover ordering and reopening.

Pages > sessions > append backup groups rolling snapshots by saved-session name
and timestamp. Named saves retain ten older snapshots by default (the retention
setting is read on each save). Each snapshot owns its tree, file order and
selection; append creates fresh page keys at the top level, and later deleting
the saved session removes its backups without touching appended copies. Exact
timestamp collisions and backwards clocks follow recorded reference behavior.
`oracle/record_session_backups.py` drives the real reference client; store tests
replay its retention sequence, and GUI session tests cover immutable media,
nested append and reopening.

The tab popup also closes one page or a group of other/left/right pages. Group
confirmation counts nested notebooks and children; confirmed closes use the
ordinary closed-page stack for undo. Tab navigation offers first/left/right/last
from the selected tab, with the reference's descent and recent-move rules.
`oracle/record_tab_actions.py` captures actual dynamic menus and questions;
model and GUI regressions replay quiet confirmation, cancellation, reverse
closing, nested undo, navigation and left/right focus preferences.

Tab rename asks "Enter the new name." with the current name. Its frozen page key
keeps the target correct while selecting another tab. Send-down moves this page,
this page and its right siblings, or only right siblings into a new notebook;
groups ask first. The rename-on-send preference optionally prompts after moving,
and cancelling that name keeps the new "pages" notebook, as the reference does.
The source pages retain their keys, open search state and queues; grouping creates
no closed-page undo entries. The real-client tab-actions recording and GUI tests
replay the accepted/cancelled prompts, tree order, nested selection and reopen.

Session snapshots also freeze each native downloader queue's options, pauses,
auxiliary state, file seeds and gallery seeds. Backup append and ordinary freshest
append/load create independent queues, remapping the highlighted queue and all
page identities. Changes or deletion of the original queues and work in another
loaded copy cannot change the saved history. Real paused URL-importer copies are
recorded by `oracle/record_session_importers.py`; store/GUI regressions cover
source deletion, multiple copies, metadata/state and all native importer kinds.

The system sleep controls enable the network engine’s clock-gap detector and set
a zero-to-sixty-second grace period. The running downloader daemon reloads these
settings; disabling detection clears its pending network wait at the next check.

Files and trash can prefix copied hashes with their booru type. The preference
applies to both the focused file and selected files, for digest, blurhash and
pixel-hash clipboard actions; only hashes available in the store are copied.

Tab menus also duplicate pages and entire nested notebooks beside the original,
with independent media and importer queues. Collapse gathers the clicked page,
it and its right siblings, or only the right siblings into one new search page,
retaining first-seen file order and removing repeated files. Its single warning
can be cancelled; accepted source tabs remain in closed-page undo. The actual
loaded-media reference recording `oracle/record_tab_harvest.py` and native GUI
regressions cover each scope, nested sources, copied selection and cancellation.

## Downloader and URL display

Network > downloaders > downloader and url display opens detached display lists
for single/nested gallery generators and media-viewer URL classes. Each list has
extended selection, name/type/display sorting, and a batch yes/no/cancel question.
The unmatched-URL checkbox belongs to the media-viewer tab. Apply atomically merges
only edited display identities; Cancel discards the draft. Other definition and
parser edits are preserved. Existing gallery pages refresh their primary choices;
"show other downloaders" exposes the secondary list and retains the selected
caption. The media viewer's top-right hover frame displays clickable URL class
names, multi-domain class labels and unmatched domains using current preferences,
including changes made while the viewer remains open. The reference's matched-link
limit and ordering are replayed by `record_downloader_display.py`.

Creating a page of pages through the chooser now gives it an initial blank
search page, as the reference does. The distinct new-notebook rename preference
opens a name prompt after creation; accepting changes that notebook's name,
cancelling retains it as "pages", and selecting another page while the prompt
is open keeps naming tied to the created notebook. The actual chooser action is
recorded by `oracle/record_notebook_creation.py` and replayed in GUI regressions.
The tag-dialog service choice disables while remembering the last used service.
Changes to these options stay staged until Apply; selecting a service tab in
a manage-tags dialog remembers it immediately when enabled, even if the tag
edits are later cancelled. New notebooks can separately prompt for a name after
the page chooser creates them.
