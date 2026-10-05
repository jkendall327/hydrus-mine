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
Options > popup notifications stages the approximate maximum width (16–256
characters, default 56) and the force-fixed-width switch (default off). Apply
persists both. Each card captures these settings when it first enters the oldest
ten shown, so existing cards retain their policy across Apply and later text or
progress updates; newly shown cards use the saved policy. Wrapped text respects
the cap and gauges reserve 90% of it. The native bold title font supplies the
character measurement.

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
  them, or forget them all (asking first); searching additions and removals,
  latest first, to toggle a historical predicate on the visible media page,
  and “clear history…” with the reference confirmation. These are two shared,
  transient frame histories, independent of each page's saved predicates.
  Opening/restoring a page and cancelling an editor add no history; committing
  an OR adds one typed entry. Undo can enter a locked page's hidden query; an
  empty notebook retains the history entry. A populated locked page retains
  its badge and files even with synchronization enabled; hiding tag namespaces
  does not change the raw predicate names in the Undo menus. With neither closed pages nor
  search history, the menu is greyed out. Actual Qt inputs and menus are in
  `oracle/fixtures/search_predicate_undo.json`; authored model/native replay
  tests await hosted CI;
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

The subscription exchange codec reads and writes complete modern reference
containers, and imports legacy type3 versions1–10 with embedded histories and old
import options converted through the reference rules. Historical single-query
subscriptions acquire checker settings; obsolete gallery identifiers become a
paused unknown downloader, with the original version-specific limit defaults. Fresh history identities
invalidate cached velocity exactly as the reference does. Modern containers retain
settings, query headers, cached example/velocity data and both URL histories.
Historical file seed-cache versions 1–7 now pass through the exchange codec's
reference upgrade rules: first-occurrence deduplication through version 4,
source-time reset through version 5, old Tumblr spelling migration through version
6, and URL/path seed construction. Integer/boolean/None version-1 notes convert
with Python's text spelling. Actual Qt direct conversion and list import/export
recordings constrain order, timestamps, statuses, counts and examples. Histories
with duplicate native identities or complex/float Python note representations
fail explicitly before staging. `oracle/record_subscription_exchange.py` records the actual Qt list
clipboard flow and reference PNG. Manage subscriptions now exposes the original clipboard/JSON/PNG transport
menus. Clipboard text and selected JSON/PNG files add permitted objects directly
to the staged list, without an intermediate review window. Nested lists preserve
their object order; known unrelated types produce the original type warning.
Each file's information/error notice finishes before the next file loads. A later
unreadable or invalid file leaves earlier accepted objects in the draft and stops
the selection; future-version failures warn once and continue subsequent files.
The last accepted subscription remains selected. JSON export asks before
overwriting an existing file. Apply
persists both URL histories and retained header examples. Reset/retry updates
file-count and example caches in both staged exports and saved settings; retries
forget the old file hashes, preserving gallery examples and velocity. Missing query histories ask the original named confirmation
before that object enters the draft; rejecting leaves it out and accepting
initialises empty histories on Apply. Rejecting or dismissing that question
continues remaining objects; the information count includes permitted objects
whose missing-log import was rejected, as in Qt. Cancel invalidates pending
imports and picker/notice callbacks. `oracle/record_subscription_import_flow.py`
records these direct menu imports, warnings, accepted prefixes and selection.
The explicit legacy exchange callback still supports its old review child;
actual import menus use the direct flow. Clipboard bitmap precedence and list
drag/drop remain absent (see DIFFERENCES.md).

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
downloader (asking which is primary and its new name). Merge stages every
group until all primary choices succeed: cancelling any primary leaves the
list unchanged; cancelling a name keeps that primary's name and continues.
Names are allocated after all absorbed owners go, with casefolded collisions
against surviving original and newly chosen names. Overlapping query texts
keep independent queue identities and both file and gallery histories on Apply,
as recorded by `oracle/record_subscription_merge.py`; cancelling the list writes
nothing. The dialog can also separate one (in
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

Folder managers temporarily pause their workers before reading a draft. If a
worker is still finishing, the existing manager window shows “Waiting for import
folders to finish.” or “Waiting for export folders to finish.”; Cancel or closing
the window abandons that wait. The draft opens after the worker commits, and its
lease lasts through Apply/Cancel and all owned children. Workers stop at the next
seed/query/copy boundary, and daemon schedules wake when management closes.
User pause preferences stay intact, including changes from another live client.
The lifecycle is recorded by `oracle/record_folder_manager_lifecycle.py` and
replayed in `tests/gui/folder_manager_lifecycle.rs`; worker exclusion also runs
through the real import/export-folder regressions.

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

The filename-tagging advanced tab now has actual quick-namespace and regex
lists in both manual-import and import-folder modes. Quick namespaces use
Add/Edit children with separate namespace and regex fields, retain literal
colons and whitespace, sort either column while preserving selection, and
ask before deleting the selected rows. Empty namespaces and uncompiled regexes
keep the child open without changing accepted rules. Enter adds a raw regex;
double-click returns the single selected regex to its input for correction,
including duplicates. Child cancellation, owner cancellation and callbacks on
closed owners cannot leak changes. Accepted rules reach local-import seed tags
and survive saving and reopening an import folder. The real Qt handlers and
filename results are recorded in `oracle/record_filename_rules.py`. Both regex
inputs also expose the exact help URLs, component clipboard phrases and shared
favourites manager. Clipboard choices preserve input text. Favourites Apply is
immediate and survives cancellation of the filename-tagging draft; cancelling
the manager preserves its stored list. Nested favourites block owner edits and
close with their owner.

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
`oracle/record_sidecar_editors.py` recorded the reference's. Router editors also
show one example-results table per source: file path or media hash, imported
texts and router-processed texts. Filename tagging, import folders and manual
exports supply up to 25 local files or media results; previews read their
sidecars or metadata without exporting. Source/formula and router processor
children inherit the reference's first-example strings. `oracle/record_sidecar_testing.py`
records seven file/media, empty-input, processing and parse-error states.
JSON destination object names are an ordered queue with add/edit text children,
up/down, deletion confirmation and double-click editing. Literal duplicate,
whitespace and embedded-newline keys survive staged Apply; empty input receives
the reference veto. Parent cancellation closes the child and rejects stale
answers. `oracle/record_sidecar_json_names.py` records 14 real queue states and
text-entry validation; the saved-router worker regression verifies nested JSON
updates preserve unrelated existing object contents.
Export folders also have **update test example files**: their current query runs
in the background, uses ascending file size when a system limit removes results,
and supplies up to 25 media results to router/source previews. The button says
**loading…**, then **got N files!**; editing the query resets it while retaining
previous examples. Closing the owner discards pending results. This is recorded
against actual reference database queries in `oracle/record_export_folder_examples.py`.
The folder and manual-export filename boxes share **pattern shortcuts**. Their
native menu copies the seven reference phrases (including the Unicode namespace
and tag placeholders) and the clickable heading to the clipboard, leaving the
current filename pattern unchanged. Pasting a copied phrase uses the existing
filename generator. `oracle/record_export_pattern_shortcuts.py` records all labels,
separators and clipboard payloads from the actual shared reference button.

Router queues also import and export selected routers through clipboard text or
reference PNGs, duplicate selected rows, and ask before deleting them. Imports
check the owner's allowed direction, source and destination types before staging
the package. Apply saves the queue with its owning import folder or export
editor; Cancel closes recursive source, formula, processor and exchange children.
`oracle/record_router_exchange.py` records both queue contexts, all six node kinds,
exact rejection messages and duplication selection. The accepted URL-to-TXT router
is also exercised through the native manual-export worker and its written sidecar.
Router and subsidiary queue PNG exports open the reusable owned **export to png**
panel. Its type/count/size summary and default title match the reference object,
and title, optional description and width reach the readable PNG header. Closing
the queue or parser closes that child and invalidates retained export callbacks.

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
collect control offers it, and an ordered namespace sorting scheme editor), file viewing statistics (whether they
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
that name and the Rust version. Raw imported empty names remain empty on Cancel;
unchanged Apply accepts the reference fallback, and field-scoped saving preserves
concurrent exit changes and externally replaced names. Main-title refresh reads
the current binding's Store and permanently stops after accepted close.
The gui page also saves the default-off switch to the main window when a viewer
tag search creates a page. Middle-clicking a single hover tag creates a real
search using its canonical identity, originating file location and saved default
tag service; display connector/underscore substitutions never become predicates.
Enabled activation requests use native focus/raise on Windows, macOS and X11,
skipping already-active native windows. Closed/re-shown viewers, stale file
payloads, hidden owners and retired main bindings refuse searches; a declined
main close leaves the producer live. Pending questions, warnings, slideshow-period
input and the viewer's advanced-delete child block retained tag-search callbacks.
The viewer's file identity is private to its original Store and canvas owner;
the canonical tag identity stays separate from the rendered label. The API
corrections add no completion credit and await exact-source hosted validation.
Wayland activation and multi-tag/child-list
activation remain unfinished. Exit and the window close button ask the recorded
yes/no question when enabled, automatically accepting after 15 seconds; declining
keeps the client open. Exit callbacks and their timeout require the current visible
main binding; retired callbacks cannot answer a successor's identical question or
close a retained window shown again. Its reference search box ("Search options... (Experimental!)"): as it
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
to a new notebook prompts to rename it. The regex child blocks parent Apply and page/search navigation; regex, shortcut
and routing children reject sibling launches while another is open. Hidden/retired owners cannot open, copy from or accept that child. Row editing
offers a read-only favourites menu from explicitly owned saved preferences,
with an enabled instruction and phrase copies that leave the input untouched.
Unsaved list rows do not become menu choices. Descriptions cannot be empty;
whitespace and advisory invalid regex fragments remain permitted.
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
The simple panel's tag lists open the shared write-tag editor on that real
service and have direct paste buttons. Entry adds existing tags rather than
toggling them; selected-file edits preserve each file's untouched tags and
spread explicitly re-entered tags across the frozen selection. Autocomplete
paste skips tags already in the selected union, while direct paste applies
all pasted tags to each selected file. Child Apply updates the filename draft;
child Cancel and owner closure discard it. The same all-files list edits and
reopens in import-folder options (`oracle/record_filename_simple.py`).

The import folder's cached-path log is an owned copy, including for a new
folder with an empty cache. Its Apply accepts complete ordered seed records
into the folder-manager draft; log Cancel discards them. Accepted cache edits
survive cancelling an existing folder's fields editor, as Qt's setter acts on
that manager-owned folder. Manager Apply writes the cache with the folder's
settings in one transaction; manager Cancel and closure reject it. Immediate
importer logs retain their live behavior (`oracle/record_import_folder_log.py`).

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
subtags in each namespace in turn (current and pending, in its independently selected tag service,
in human order, as the reference's `GetComparableNamespaceSlice`); a rating
sort by the rating, unrated files counting as -1 (0 on an inc/dec
service). Under the sort control, the collect control ("no collections",
or "collect by series-stars") opens the reference's choices: the
namespaces in your namespace sorts, then the like/dislike and numerical
rating services, each checked to collect by it; its ⚙ chooses its own tag service and whether files
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
buttons. The hash editor now presents vertical is/is-not and SHA-256/MD5/SHA-1/
SHA-512 radio groups around the multiline input, with both cleanup buttons
stacked underneath. Arrow keys stop at each group's edges, Space selects, and
Enter accepts using the existing default radio-key convention. Cleanup replaces
the live typed input even when removing every bad line; normal cleanup preserves
bad or mixed input and opens an owned Warning/OK notice. That notice blocks
editing and acceptance until acknowledged, and closes with its parent on Cancel,
rebind or accepted client exit. Forced cleanup keeps its existing "You sure?"
yes/no draft question. Typed inclusive/exclusive queries, explicit reconstruction,
Cancel, saved-default reopening and ownership boundaries have authored regressions
against `oracle/record_hash_predicate.py` (actual Qt controls, warning PNG, ten
file queries and twelve cleanup cases); hosted Rust execution and native rendered
review remain pending. `system:rating` has a panel for each rating service (like or
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
or, with "forget", forgotten. Yours come across from hydrus. Each panel's star
opens "set this as new default", with "reset to original default" when its
comparable family has a saved value. Saving stores canonical typed predicates
immediately and affects future editors; closing or cancelling the current owner
keeps that change. As in the reference, star Save bypasses the regex panel's
acceptance validation; an invalid saved regex initializes future fields but OK
rejects it until corrected. Reset also commits immediately and leaves current fields
untouched. Explicit compatible typed input takes precedence. Date/relative time,
views/viewtime and the four URL-rule types have independent defaults. Rating
comparability spans service keys, as the reference does; per-service rating
panels expose save/reset but retain their built-in initial values, while advanced
rating uses saved defaults. A number-of-tags shortcut that emits a namespace is
saved without becoming a number-of-tags default. All 40 panel families and 1,600
comparability pairs are recorded by `oracle/record_predicate_custom_defaults.py`.
Imported defaults use the same typed store setting, including actual numerical
rating service scales. Reset remains durable when the store is reopened.
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
export phrase, and removes selected rows after asking. New panels open at the
**exporting > export folder > Default export directory** option. Its path and
browse control wait for options Apply; Cancel discards them. Empty or whitespace
uses `hydrus_export` in the home directory, and a one-off manual destination does
not replace this default. Legacy portable paths resolve against the source
database; native relative entries resolve against its database directory.
`oracle/record_export_default_directory.py` records these option and consumer
boundaries. The
existing sidecar routers editor supplies tags, notes, URLs and timestamps.
Copies overwrite existing destinations; links are optional. Export runs on a
worker with progress and cancellation between files. Trashing asks the
reference's confirmation and disables links; "export and close" asks "Export
as shown?" and closes after success. `oracle/record_export_files.py` records
the reference panel's previews, removal and confirmations, and its export
worker's collision filenames and overwritten file contents.
The read-only "files' tags" sidebar shows actual display tags for selected
files, falling back to all kept files, with current, pending and petitioned
counts. Local tag/subtag/count sorting preserves selected tags and remembers
text/count order separately. Ctrl+C copies selected raw tags; context menus
copy raw/subtags/counts, open native AND/OR/separate search or duplicate pages,
edit relationships, and persist favourites/most-used choices. Middle-click
launches a search (Shift uses OR); double-click has no action. Closing the owner
invalidates pending menu answers and nested relationship editors.
`oracle/record_export_selected_tags.py` records these actions on the actual
reference panel with controlled pending/petitioned mappings in the basic store.
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
a new one, and "delete current note" asks first. The cog chooses all/current
note copying and JSON/human text, starting new editors at the beginning/end,
and viewer middle-click body-only/title-and-body copying. These four live
preferences persist immediately, including after note-editor Cancel.
The cog opens through normal button activation, refreshing checks from saved
preferences each time; right-click does not open it, as in Qt.
"paste" merges JSON notes in as the
reference does (extending a note a pasted one extends, renaming on a
clash; text it can't read is said so), and "copy URLs" copies the URLs
in the note in view. "apply" writes the notes (each cleaned, empty ones
dropped) and deletes the ones gone; "cancel" with changes asks first.
Options → notes stages the two cursor/hover switches until Apply, preserving
intervening live cog choices. Imported preferences survive reopening. Existing
note tabs retain their real cursor/selection when switching or flipping the cog;
new and pasted tabs use the latest initial-cursor preference. Middle-clicking a
viewer note copies using the latest saved choice without reopening the viewer.

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
(as the warning under it says). Disk updates run in an owned background worker
on the accepted ordered targets, with cancellation between files. Its
"setting file modified dates" popup appears only after three seconds have
passed; cancellation retains the database updates and completed disk prefix.
Closing or replacing the date child cancels its draft, and retired dialog
callbacks cannot apply it or clear a successor.

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
then shows and searches it as that type, as the reference does. The accepted
operation runs in a background worker, committing and moving in blocks of 64.
Above 64 files, the existing popup shows "forcing filetypes" and allows
cancellation before the next block. Rename failure preserves a copied file's
mtime and queues duplicate-extension cleanup one hour later. Imported shared
media remains copy-only. The accepted file targets are frozen, so changing the
current selection cannot redirect the work. `oracle/record_metadata_file_jobs.py`
records these bounded filesystem/popup paths and both rejected dialogs; the
two original worker leaves remain Partial for the platform and lifecycle
limits documented in DIFFERENCES.

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
(`src/still.rs`; the filters do the same). Closing a canvas releases its
resampling worker even when a caller retains the closed window handle. If
the OS cannot start that worker, the existing image stays visible with
nearest-pixel scaling; a changed file, zoom or clipping plan can retry.
Multiple viewer windows remain independently closable without clearing a
successor's current-viewer slot. F3 manages the
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
delete M from the chosen local domains: enter commits (archiving the kept,
deleting the deleted, with the delete lock's "inbox deletees" option
honoured), f asks to forget, escape goes back to filtering. The deleted leave
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

Options > duplicates > duplicates filter page includes the reference's
"Hide the \"x% done\" notification on preparation tab when >99% searched"
checkbox, enabled by default. Apply updates the current preparation label;
future pages and reopened stores use the saved value. Exactly 99% still shows
the percentage, while 99.95% shows "99.9% done" when suppression is disabled.
The switch changes the label alone, preserving counts, the gauge and search
availability. Drafts stay local to Options, and Cancel, hidden or retired
callbacks cannot save them. Imported and retained legacy ClientOptions use
the same Boolean. `oracle/record_duplicates_progress_option.py` records the
actual control's save/Cancel/reopen and both policies at thirteen preparation
boundaries; authored store/model/native regressions await hosted execution.

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
(the top one's zoom options, volume, shortcuts and deeper view-options actions,
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

Timestamp content parsers offer the reference's single **source time** choice.
Saving normalises unset or obsolete timestamp types to modified-domain/source
time while preserving the formula, processing steps and auxiliary data. The
actual parser returns typed timestamp metadata; date conversions, earliest-time
selection and future-time clamping feed the downloader's real file-seed source
time. `oracle/record_content_time.py` records the control and metadata, including
unset/obsolete inputs and date-converted documents. Apply stages through the
content owner; stale callbacks cannot change a saved parser after closure.

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

Services → review opens the service registry with each local and built-in service's name, type, database id, service key copying and refresh. File-domain sizes and deleted counts, tag mapping/tag/file counts, and rated-file counts come from a consistent native store read and match `oracle/record_services.py`. Refresh retains the selected key. Local trash review now offers confirmed clear/undelete actions and local rating review offers confirmed deleted/non-local/all-file clearing; unavailable remote administration actions retain an explicit explanation.

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


The tag autocomplete tabs options page now opens a shared favourite-tag list
editor with suggestions, manual fetch, domain controls and add-only manual/paste
entry. Removing tags and applying the child updates the parent options draft;
parent Apply saves the naturally sorted global list for every favourites tab.
Child Cancel, parent Cancel and owner close discard the corresponding draft.

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
content nodes. Subsidiary rows support add, edit and delete, including recursive child pages,
separation formulae and source-time sorting. The queue supports extended selection,
duplicate, confirmed deletion and standalone wrapper clipboard/PNG import/export.
Imported wrappers retain separator/sort settings, recursive pages, parser keys and
reference editor context; changes stay staged until the owning page/list applies.
Child content and recursive editors
receive separated examples with their source URLs and context variables.
Separation formula editors receive the raw inherited document and preserve
parsed newlines. All edits stay staged until their page and parser list apply;
closing an owner cancels every descendant. Content editors support URLs, tags, notes, hashes, timestamps,
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

Login definitions have typed script, step, credential, cookie and example-domain
representations. The bounded interchange codec reads/writes reference login script
JSON and compressed PNG, upgrades old fixed cookie names, and retains matcher
and formula editor data. `oracle/record_login_editors.py` records the actual Qt
credential panels, script-list import/rename and script validation. Network >
logins > login scripts opens a staged script list with add/edit/delete, extended
selection and JSON/PNG exchange. Script edits preserve their keys; additions and
imports regenerate keys and make names unique. Credential definitions expose their
name, normal/password presentation and shared permitted-input matcher. Credential
entry masks passwords, validates live and reproduces the advisory acceptance
question for blank or invalid values. Child edits wait for the owning script and
script-list Apply; closing a parent cancels its children. The step queue supports add/edit/delete and order changes. Each step edits name,
scheme, method, optional subdomain and path, with the reference subdomain/path
cleanup. Its response content list supports extended selection, unique named
VARIABLE/VETO nodes, shared formula/live preview, delete confirmation and reviewed
JSON/PNG exchange. Parent cancellation discards every nested formula/parser.
Credential/static/temporary argument dictionaries and required-cookie matchers
are editable and consumed by real HTTP attempts. Script example-domain rows support extended selection and add/edit/confirmed
delete. Their editor asks domain, access and description in the reference order.

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

The login-script test panel now uses the shared network-job control for its live
isolated request, with URL copy, context bandwidth-rule editors, phase-specific
retry commands, cancellation and a retained five-second auto-override policy.
Login requests bypass request-admission bandwidth limits immediately while still
counting usage; response-body throttling still applies.
An explicitly supplied control error can be shown or copied and survives clearing
the job; a failed login alone does not invent a control error. Completion opens
an owned Information notice with the exact result and an OK button before the
run button is re-enabled and the final label changes. Parent Cancel retires the
notice and any rules draft; retained actions cannot reach a later request or run.
The help menu opens the existing local `docs/downloader_login.md` page through
the ordinary OS launcher.

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


## Tag migration

Tags > migrate, service review's local/repository tag pages and Manage Tags' selected files open
"migrate tags…". The global entry uses the configured or remembered default
tag-dialog service and has no selected-file restriction. Choose mappings,
siblings or parents; a real source and
destination; current/deleted source content (also pending or current and pending
for repositories); and the actions available for the destination. Local services
support add/delete, plus clear deletion records for mappings. Repository actions
pend/petition local proposals, with an editable petition reason. Mapping scopes
use selected files or the existing multiple/current/deleted file-domain selector.
Mappings and each side of pairs use the reusable tag-filter editor. Sources and
destinations also offer Hydrus Tag Archive (mappings) or Hydrus Tag Pair Archive
(siblings/parents), with native file pickers and read-only path/type inspection.
Existing destinations keep their hash/pair type and merge additions; new mapping
archives offer SHA256, MD5, SHA1 and SHA512. Known alternate hashes convert through
the stored digests; unavailable conversions are skipped. Selected-file and
current/deleted domain scopes apply through SHA256 even between alternate archives.
Pair filters can require real current or pending mappings on the left, right or
either side in a chosen tag service. For siblings, the right test follows its
chain to the terminal ideal; either-side mode disables the individual tests.

The window shows the reference summary and its second confirmation outside
advanced mode. Migration runs on a worker in bounded atomic batches, with live
progress in an independently retained popup, with the reference preparation,
rows/s, cleanup and done text. Closing settings leaves its published job running.
Pausing displays "paused" and waits after the current committed batch; cancellation
wakes paused work, removes its controls immediately, and retains committed batches.
A running popup cannot be dismissed; cancelling permits dismissal while cleanup
finishes. Completed popups disappear after the reference three-second integer
deadline or can be dismissed manually.
Services are resolved by key again on every batch. Graph/count publication occurs atomically per batch; displayed tags and review
counts refresh after completion or cancellation. Reference controls, questions and actual DB mapping
and pair destinations are recorded in `oracle/fixtures/tag_migration.json`.
`oracle/fixtures/tag_archives.json` records actual Qt archive inspectors and
confirmations, 32 all-known/selected hash-conversion combinations and eight
current/deleted domain cases, current/pending count
gates, and real Python → native SQLite archive codec → Python readback for all
four mapping kinds and both pair kinds. Pause/resume/cancel uses the real Qt popup
and MigrationJob in `oracle/fixtures/tag_migration_pause.json`. The actual Qt
popup lifecycle, closing settings independently, exact phases/speed and cancel/
dismissal states are recorded in `oracle/fixtures/tag_migration_progress.json`.

## Downloader definition interchange

Network > downloaders > import/export downloaders exchanges URL classes,
page parsers, registered login scripts and single/nested gallery URL generators with the reference
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
The mixed exporter offers registered component selection, including login-only
packages and linked generator/class/parser dependencies, through clipboard JSON,
JSON files and PNG files. Login scripts contain credential definitions, cookie
rules, ordered requests and response parsers; saved credentials, sessions and
domain activation stay in the store. Mixed import reviews login scripts beside
the other components, skips exact script duplicates without comparing their
name/key, and regenerates new identities. Existing matching-name example-domain
links follow imported scripts while retaining current credentials, validity,
activation and delays; unconfigured example domains remain unconfigured.
`record_mixed_login_packages.py` drives the actual Qt registered-script chooser,
dependency expansion, PNG export and mixed import. Native codec/model/window
replays cover saved script-list consumers, concurrent domain changes, stale
script lists and retired callbacks; these new assertions await hosted CI.

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

File Search options can start new search pages paused or searching immediately,
and show or hide `system:everything` in read autocomplete. Apply persists both
preferences; Cancel discards edits. Existing pages retain their pause state, and
resuming a paused page executes the query it has accumulated.

Tab context menus now append a saved session inside the clicked tab row's
notebook. Clicking a page of pages also offers saving its contents to an existing
non-reserved session or creating a new one, with that notebook's name suggested.
The overwrite, duplicate-name, reserved-name and cancellation steps follow the
actual reference dialog recording in `oracle/record_notebook_sessions.py`.
Saving targets the clicked notebook's key, independently of the selected sibling;
its wrapper is omitted from the saved tree. Copies retain independent page media,
selection and importer snapshots. Appending into a background notebook remembers
its new child selection while preserving the visible sibling, and survives reopen.

The network engine's local job-control protocol distinguishes connection retries, server bandwidth retries, domain errors and gallery waits. Commands validate the daemon epoch and live request identifier. A recent-error journal lets the owning control display a failure even after its request leaves the live list. The reference recording also establishes that the visible error menu contains only show and copy; clearing belongs to the control's owner.

The File Search default/fallback local location button edits current importable
file domains in a child selector, including multiple domains. Its Apply stages
the location in Options; Options Apply persists it. Blank search pages and new
notebooks use it, as does switching an all-known-file search to all known tags.
Missing services are removed; an empty default resolves to all local file domains.

Manage Tags now uses the shared write-autocomplete model: known zero-count tags
stay available, typed tags and their ideal sibling are elevated, counts follow
the configured write domain and tag service, and parent/sibling decorations use
the edited service. Ctrl+Space fetches when automatic fetching is disabled.
Suggestions scroll without the old twelve-result cap and can be entered by
clicking. Tag Editing's six autocomplete controls set the first counted result,
multiline paste confirmation, parent/sibling decorations, expanded parent rows,
and visible list height (1–128 rows). Clipboard paste adds cleaned unique tags
without toggling existing tags off; its question and all staged tags are dropped
when the owner is cancelled or closed. The running Qt client's synthetic corpus,
rendered rows and paste decisions are in `write_tag_autocomplete.json`.

Read autocomplete now captures the File Search active-predicate height (6 rows
by default), results height (22 rows), and floating policy when each page is
created. Both heights accept 1–128 text rows and scroll additional entries. The
active predicates appear above the search input. Floating results overlay the
page while the input is focused; embedded results reserve sidebar space.

The tab popup refreshes a leaf or every initialized descendant of a notebook,
without changing the selected page. Paused searches resume, locked searches stay
fixed, and importer thumbnails reapply their current sort while retaining
selection. Empty notebooks omit the refresh action. Advanced mode adds a copyable
page-weight label for the clicked subtree: each child contributes its file count
and each importer file or gallery seed contributes twenty, including repeated
files shown in separate children. `oracle/record_tab_refresh.py` records the real
recursive dispatch, search states, weights and clipboard text.

The sibling and parent editors now share the same write input on both sides.
Each service retains its own typed drafts, suggestions and highlighted result;
keyboard entry and clicked suggestions use the selected tag, Ctrl+Space forces
fetch, and pasted tags only add to selections. Pasting a tag on the opposite
side removes it from the original side. A pending paste cannot apply the
relationship editor; closing its owner invalidates subsequent answers. The real
Qt sibling/parent preview selections are recorded alongside write suggestions.

Subscription import options can be copied as the reference JSON container,
pasted into selected subscriptions and cleared after confirmation. Clipboard
changes remain staged until Apply. The subscription popup preserves v688's
observed callback routing: merge-paste replaces the slice, fill-in-gaps-paste
merges incoming custom kinds, and replace-paste fills currently inherited kinds.
The real Qt recording is `oracle/record_subscription_import_options.py`; native
regressions cover clipboard output, all three modes, invalid input, declined
clearing, reopening and callbacks retained after the owner closes.
The recorded containers also exercise both blacklist and whitelist rules in
global tag filtering, parsed-tag filtering and the existing-tag test. Text/PNG
interchange retains their reference rule codes and behavior.

File Search can set an implicit search limit (none by default; 1–100,000,000).
The shared query engine applies it only without an explicit `system:limit`, so an
explicit larger limit overrides it. The sort-refresh preference defaults on:
changing a supported database sort reruns a synchronized, explicitly limited
local search to choose its new sorted subset. Paused searches, implicit-only
limits, all-known-file locations, and unsupported sorts keep the current subset.

Import options' additional-tags and file whitelist lists can also be edited in
a detached shared write-tag window. It uses the additional-tags service or all
known tags for whitelist suggestions. Typed entry toggles a listed tag, paste
only adds, and Apply returns the accepted list to the parent options draft.
Child Cancel/native close preserves the caller's list, unlocks the parent, and
parent closure cancels the child and invalidates its pending answers.

Tab popups offer “new page” for the clicked row's notebook and “new page here”
before the clicked tab. The chooser keeps those destination keys when selection
changes, and cancellation clears the pending insertion. Right-clicking a row's
unused space also opens its new-page action. GUI Pages exposes “Put new page tabs
on” with all four reference choices; legacy preferences import and applied changes
reach new-page creation immediately, while an explicit “here” position overrides
the preference. The real chooser outputs for all positions and cancellation are
recorded in `oracle/record_tab_new_page.py`.

The file and search download controls on URL, simple downloader, gallery and
watcher pages now have the reference cog menu. Its decoded URL label copies the
original URL, and its bandwidth-rules submenu opens the existing detached rule
editor directly. Default contexts appear once; temporary page instances never
get their own rule editor. Applicable actions reattempt a connection or a server
bandwidth wait, scrub domain errors, override this job's bandwidth, or skip this
job's gallery wait. The five-second automatic override belongs to that page's
file/search control and survives changing the highlighted query. Closing a page
retires its policy; accepted overrides remain in effect for the current job.

A failed request leaves a last-error button on the owning page control. Its menu
has only **show error** and **copy error**. The error window uses the reference
**Network Error** title and preserves the text. Removing a finished job keeps its
error; the owner's explicit clear hides the button and suppresses replay of the
same stored failure. The page-parser example-fetch owner clears after validating
a new URL and retains the completed failure after removing its live job. Empty
URLs and raw test-panel fetches preserve the owner’s error; closing its editor
closes the error window.

Importer option editors now expose reference container copy/paste and the shared
custom overwrite chooser. The chooser shows current, pasted/loaded and result
columns, with merge, fill-in-gaps and replace presets plus per-kind checkboxes.
Applying the child replaces only the parent draft; closing either owner cancels
uncommitted edits and retained callbacks cannot apply twice. Non-full replacement
of global options is rejected with the reference information message.

When several controls review the same request, each automatic override has its
own owner. Closing or unchecking one control leaves another control's policy
intact. The override also skips a data-speed wait after five seconds and counts
the transferred data normally.

The current-network-jobs review shows the same selected-job control, including
context rules, applicable retry/wait actions, and show/copy for a failure retained
after the selected request disappears. Its automatic policy has an independent
owner, so unchecking a page control does not disable the review's policy.

Page parser example-data fetching retains its network failure after the job ends.
The error menu shows the full native diagnostic or copies it unchanged; the next
validated example request clears the retained failure. Closing the parser also
closes its error popup and invalidates retained callbacks. Raw formula/content
URL fetching uses its own test text, as in the reference.

Parser and formula raw-data panels copy the complete document and paste into the
current example while retaining its context. Read-only previews format JSON with
the reference's four-space indentation, describe JSON/HTML character counts and
clip display text at 500,000 characters without clipping parser input. Fetches
inspect response bytes through the existing media engine; recognized media shows
its type and **no preview**, with test parse disabled. Changing examples restores
their detected type, while a pasted replacement clears it. Recursive children
inherit the document and context, matching Qt's text-only child data contract.
`oracle/record_parser_raw_preview.py` records these controls and exact wording.


Network > logins > logins opens preserved or saved domain entries. Edit credentials
uses the script's definition order and masks passwords, with the recorded live
labels and advisory invalid/blank confirmation. Accepted values reset delays and
validity, deactivate invalid credentials and ask before activating a valid inactive
domain. Domain Apply saves its draft while preserving concurrent script edits;
Cancel closes credential children and ignores stale handles. Flip active, scrub
delays and scrub invalidity work on extended selection. The logged-in column reads
required cookies from the shared session store, showing session lifetime or the
earliest required-cookie expiry; delays use the reference relative expiry text.
Reset login asks the recorded irreversible-delete question, then clears the
selected domains’ resolved sessions immediately, even if the manager is later
canceled. Other sessions remain intact and existing HTTP engines see the reset
on their next request. Cookie rows and action eligibility refresh while open.
Add offers available example domains or the recorded custom-domain/access/description
chain, then credential entry and optional activation. Change login script groups
matching examples first with a selectable no-op separator/current-script entry,
preserves credentials and valid activation, and resets invalidity/delays. The final
description Cancel retains its default; parent Cancel discards unfinished children.
Delete confirms “Remove all selected?” and stages removal until Apply. Do login now filters
selected active, non-invalid, existing-script domains whose required cookies are
missing, asks the recorded confirmation, saves the domain draft and closes the
manager before attempting its sorted queue. Attempts use the existing cookie store;
key-guarded results update validity or network delays. Reopening the manager shows
HTTP/final status and can cancel the queue. Canceling the application owner stops
remaining attempts.

Shared write-tag inputs now offer tags, favourites and children in Manage Tags,
both sibling/parent inputs, and detached import additional-tags/whitelist
editors. Favourites use the configured count service's sibling/parent
decorations. Children follow the current selected tags, remove already-present
tags, sort by display counts and show countless rows. The Tag autocomplete tabs
option limits children to 40 by default; a staged limit or “show all” reaches
open consumers only after Apply. `oracle/record_write_tag_autocomplete.py`
drives the real Qt favourites decorator worker, children database query and
noneable limit control; model and native-window regressions replay their output.
GUI Sessions now applies the autosave period and idle-only preference to a
historical save timer alongside live session synchronization. Active idle-only
saves retry in sixty seconds; eligible saves use the configured one-to-1440-minute
period. Unchanged session data, including selection-only changes, creates no new
backup. Main-window key/pointer activity and page commands from the Client API
feed the imported user/mouse/API idle timeouts, including the initial two-minute
boot guard. Switching the startup preference away from last session stops the
scheduled save chain after its next eligible tick. The real controller's cadence,
unchanged hash suppression, history and idle boundaries are recorded in
`oracle/record_session_autosave.py`.
Media playback Options now control whether a resized viewer restores its default
fit or keeps the current detail zoom and pan. Transparent media can show the
reference's 16-pixel checkerboard or bright green background; opaque files keep
the ordinary canvas background. Media viewer Options set the seek bar's full
height (1–255 pixels), mouse-away height (1–255 pixels or completely hidden),
and nub width (1–63 pixels). Apply refreshes an open viewer; Cancel discards the
draft. The configured nub width also determines where clicks and drags seek.
Legacy preferences migrate with the reference's defaults. These controls and
native pixels/geometry are recorded in `viewer_canvas_options.json`.
The file log’s **ADVANCED: import new sources > from clipboard** imports
nonblank lines immediately, applies the active URL classes, and skips duplicate
seeds without retrying failed ones. The first line determines URL or path type
for the whole batch, as in the reference. Empty, unavailable, or inaccessible
clipboard text displays an error and preserves the existing log. Closing or
replacing a log invalidates its old callbacks.

The selected-row **search for URLs** action opens a **url search** page in local
file domains with one OR container of exact URL predicates. It uses the selected
seeds’ request URLs and executes the search through the existing search engine;
the page and its predicates can be saved in a session.

Shared write-tag suggestion lists now have a right-click menu for copying raw
tags, subtags, underscores, counts, all list tags and tags with parents. Parent
and sibling display toggles affect only the current widget/tab and reset on
reopening. The favourites submenu persists additions immediately and asks before
removing a favourite or a service-specific most-used tag; declining or closing
the owner leaves settings unchanged. Menu questions block owner Apply, and
confirmed writes reread settings to preserve changes from other windows. The
open submenu hands selected raw predicates to the main window's weak search/
duplicate launcher. The real Qt menu actions, copy payloads, questions and page
publications are recorded by `oracle/record_write_tag_autocomplete.py`; model
replay and native-window lifecycle regressions cover these boundaries.

The login HTTP module runs ordered GET/POST steps through NetEngine and the normal
persisted cookie store. Static arguments, credentials and temporary variables
follow the reference's precedence. POST sends form data and a previous-step
Referer/Origin, response parsers transfer variables or veto, and step/final cookie
matchers decide success. Results retain each raw URL/body preview, downloaded text,
new variables/cookies and status. `oracle/record_login_execution.py` actually runs
the reference HTTP jobs on a loopback-only dummy site and records success, missing
cookies/variables, veto, final mismatch, HTTP 401 and cancellation. Native scoped
HTTP regressions replay those results, inspect wire requests, reopen session cookies
and cancel an active request. Script tests use an isolated cookie store while real
domain runs share persisted sessions. Completed step rows stream into the owning
script window before the next wait/request and can be reviewed while it is running.
Cancel retains completed results and stops later work; owner closure invalidates
queued result callbacks. Run test first asks “Edit the domain.” with the first
sorted example or remembered domain, followed by remembered credentials. Domain
Cancel/blank or credential Cancel launches no request and retains existing results;
only starting an accepted run clears them. Closing the script retires its prompt.

Import-option editors now have the reference favourites/profiles star menu:
load, custom load, copy, edit/add and confirmed deletion. Favourite editors
include their name and template description; they permit loading and copying
other profiles while preventing recursive profile edits. Save-current prompts
for a name, and collisions follow Hydrus's suffix rules. Saves and deletions
persist immediately, preserving other owners' defaults. Custom load and profile
editing lock the parent draft; Cancel or owner closure invalidates retained child
callbacks. The real Qt menu tree, prompts, editor outputs and acceptance/cancel
paths are recorded in `subscription_import_options.json`.

GUI Sessions also selects the default session on startup: a blank page, last
session or any saved name. Choices remain fixed while the options window is
open, and Cancel leaves the startup preference unchanged. A missing session
name falls back to a blank page. Named sessions restore their saved tree, ordered
media, selection and independent importer state before the main window opens;
`oracle/record_session_startup.py` records the four ordinary startup outcomes.
File-log source PNG exchange now imports reference carriers through **from png**
and exports the frozen source lines through **to png**. The shared **export to
png** panel has title, payload description, description, width (100–4096) and
path controls, validates the path/title before writing, appends `.png`, shows
**done!** briefly, and remembers the successful export directory. Exports have a
readable image header and compressed UTF-8 payload compatible with the reference.
Cancelling an import picker or closing an export leaves the log unchanged;
malformed or oversized input displays an error. The export child closes with its
log and rejects callbacks through a retained old handle.

Media viewer hovers Options now independently enable or disable the tags,
ratings/locations, and notes pop-in panels. The passive bottom-right index
preference draws the current zoom and index as “zoom - index”, three pixels from
the canvas edge, underneath the media. All four checkboxes default enabled, as in
hydrus; Apply updates the current viewer, while Cancel retains its settings.
The reference recording `viewer_hover_options.json` covers the actual hover
layout gates and background draw calls, including independent combinations.
The shared suggestion menu also shows sibling ideals and parent/child lookups
from all real tag services, grouped by common service membership with the
reference's ten-item display cap. Add siblings/parents opens an owned relationship
editor seeded on every service tab. Those dialogs now select the configured
default service and remember real tab changes immediately, including after
Cancel; disabling memory preserves the configured default. Closing the write-tag
owner cancels its relationship child and prevents stale Apply. The reference
recorder runs the actual relationship lookup and initialization workers and
records all service seeds and preference changes; model and native child tests
check graph publication, cancellation and owner lifetimes.

Write-tag menus now open real search or duplicate-filter pages through the main
window. The selected raw tag predicates, file domain, default tag service and
reference page names reach their query consumers and persist in sessions.
Parent decorations remain display text and are not silently added to searches.
The launcher holds weak owner handles; closing a tag input invalidates its menu.

Idle activity now also follows native input events in auxiliary windows. Mouse
movement updates the mouse timer; keys, clicks, scrolling and opening/focusing
an editor update the user timer. Rendering and losing focus do not reset either
timer. The reference's independent dialog, mouse and API timestamp updates are
recorded by `oracle/record_session_activity.py`.
and cancel an active request. The script editor now has a domain field, run/cancel
controls, current HTTP status, final result and a results table. Run asks for its
credential values and uses a fresh cookie store, leaving the client sessions and
staged definitions unchanged. Applying while a test runs is blocked. Review opens
read-only URL/body/data/variable/cookie/result fields; the data preview contains
at most 1024 Unicode characters, and copy transfers the complete response.
Closing the owner cancels the run and its result review, and stale actions do nothing.

Client API requests now publish an atomic timestamp-only activity marker before
authentication, including rejected requests and database-busy responses. The GUI
reads it when evaluating idle, so ordinary API reads also postpone idle-only
session autosaves. This IPC remains available while the API's SQLite pool is
paused and records no request paths, credentials or payloads.

Importing a reference database whose startup preference selects a named session
keeps that original name as an immutable snapshot as well as the native live
last session. Startup can therefore load the configured name after conversion;
its media and importer logs survive replacement of the initially staged pages.

Media viewer mouse behaviour Options can reject mouse drag-panning on files
with duration, while leaving still images, keyboard panning and seek bars
available. The cursor-hiding preference blanks the native cursor on accepted
drag movement; releasing retains the blank cursor until ordinary movement
restores it. Both settings apply to the open viewer, persist on Apply and
remain unchanged on Cancel. Duration drag blocking defaults off; cursor hiding
defaults on except macOS, matching the reference. `viewer_pointer_options.json`
records real Qt drag acceptance, geometry and cursor transitions.
GUI Sessions exposes the large-session warning preference. When active page
weight exceeds 10,000,000, the client adds the reference's warning text to the
popup stack once per boot. The weight includes twenty per importer seed and
excludes closed pages. Disabling the preference suppresses the warning without
using that boot's allowance; dismissing a warning does not cause it to repeat.
`oracle/record_session_warning.py` records the threshold, disabled state, seed
weighting and exact popup messages from the reference's live menu-count updater.
Every shared write input now has file/tag-domain buttons. Their checked menus
follow the reference and always offer all known files; choosing that domain
while searching all tags switches to the first local tag service. Choosing all
tags while searching all known files restores the default local file domain.
These domains stay local to the widget and service draft, including across
Manage Tags service changes, and feed live search, favourites decorators and
children counts. Multiple/deleted locations opens an owned staged selector;
Cancel preserves the current input domains and owner close discards the child.
The recorder drives the real Qt domain buttons and interlocks, and model/native
regressions verify checked menus, labels, counts, no options writes and child
cancellation.
The file log’s **advanced** menu exports selected import objects as the reference
SerialisableList/FileSeed JSON, including progress, headers, hashes, tags, source
URLs and notes. **re-normalise all URLs** asks the reference’s full confirmation
question in the log and downloader list menus. Yes reads current URL classes and
atomically updates request/comparison URLs, discarding later duplicates while
preserving the first seed’s identifier, status, timestamps and metadata. No or
closing the log preserves the existing entries. The reference has no advanced
object-import action.

PNG export headers also render on installations without system fonts using a
bundled Open Sans fallback with its Apache 2.0 licence and copyright notice.

Login step request arguments use three independent lists for credentials, static
variables and temporary variables. Each has extended selection and add/edit/delete
controls. Add/Edit asks the key and then the value in owned text dialogs,
using the reference prompts and defaults. Cancel or a blank key aborts; a
duplicate key warns before any value dialog opens, while values can be blank. Rename,
confirmed bulk deletion within that dictionary, row Cancel and parent Cancel
preserve the expected draft boundaries. The body scrolls and the footer stays visible. The loopback consumer regression edits a static query argument through
the native step window and observes it on the actual HTTP request.

Subscription import-options favourites now use the shared star menu. Loading
replaces the selected subscriptions' staged options; custom loading shows the
reference's multiple-selection information before opening the shared three-column
overwrite chooser. Cancel preserves the subscription draft, and Apply writes the
chosen result to every selected subscription, including the reference's behavior
after its topmost-selection warning. Profile edits persist independently of the
subscription dialog. The list offers no save-current action, and closing it cancels
its open profile/overwrite children and invalidates retained callbacks.

Scripts and steps open a shared required-cookie editor with sorted name/value
matcher rows, extended selection, add/edit and confirmed delete. Each matcher uses
the existing live permitted-input editor. A row and its whole list can be canceled
independently; accepted lists remain staged beneath their step/script/list owners.
Matcher updates persist through reopening, and the HTTP consumer regression shows
an edited cookie value rejecting the actual loopback response cookie.

An interrupted native client boot now triggers the reference's startup recovery
question when the configured session exists. It offers that session or a blank
page and automatically chooses the session after fifteen seconds. Closing the
question chooses blank; this does not change the saved startup preference. The
question retains its displayed session name if preferences change while it is
open. The native running marker clears only after saving and stopping owned work;
cancelling the password gate also clears it without opening a session. The real
recovery questions, outputs and a fifteen-second auto-yes dialog are recorded
in `oracle/record_session_startup.py`.
Search logs export all URLs through the reusable PNG panel, import new URL
lines from the clipboard or PNG, and copy complete selected page objects as
reference JSON. Imports ask the exact duplicate and continuation questions
before committing and waking the queue. The current reference's duplicate
“add all” answer ends the import; “only add new” proceeds to the continuation
choice. Each imported page gets its own run token and the selected continuation
flag. The standalone `SearchLogImportWindow` keeps the actual question text and
button labels, handles invalid PNG/clipboard access errors, and cancels with its
log owner. These actions also work directly from a downloader's search-log menu
without opening the log editor.

Seek-bar and hover pop-in focus preferences now consume actual desktop window
activation through the shared native observer. Both default enabled and are
independent: an inactive viewer keeps the seek bar at its configured away height,
while an accepted scrub or existing volume popup keeps it full. The hover rule
covers the top information, tags, ratings/locations and notes panels. With no
active application window, an already-raised hover stays up while the pointer
remains there; leaving hides it, and moving back cannot raise it until the viewer
is active. Another active application window hides it. Apply updates the current
viewer, Cancel retains the preferences, and legacy values migrate. The fresh
`viewer_focus_options.json` recording includes real Qt activation and all four
no-active-window transitions.


Example domains require unique nonblank names, preserve the entered spelling and
show all four reference access types. A changed access type supplies its default
description; unchanged access keeps the existing description. Canceling domain or
access discards the row, while Keep description (or closing that final prompt)
accepts the domain/access using its current/default description. Parent cancellation
discards the whole example draft. Before the first test, edits update a test domain
that still contains the initial example default; a previously used domain is retained.

In advanced mode, an existing subscription query's “quality info” menu can show
its saved log's inbox/archive/deleted counts and copy the exact reference CSV.
The read runs in the background, disables editor changes until publication,
and uses current file locations rather than seed import status. Repeated hashes
count once; trash and nonlocal hashes count as deleted. The good ratio excludes
inbox files. Unsaved queries do not enable the menu, and reports never apply the
subscription draft. Closing the editor cancels publication; missing saved logs
produce an acknowledgement and restore the editor.

The network > pause menu saves “always boot the client with paused network traffic” separately from the live traffic pause. The checked preference is available in basic and advanced mode, survives reopening, and is imported from legacy client options. Startup applies it before GUI daemon or standalone server workers start. Resuming live traffic keeps the next-boot preference; creating parser/login engines or restarting an attached daemon does not apply it again. The reference has no corresponding Options checkbox or Apply/Cancel draft.

Tag Editing > tag dialogs now stages and saves the service-navigation and
storage-list defaults. Manage Tags, siblings and parents use horizontal service
tabs or a vertical service list according to the listbook preference. Manage
Tags captures the separate parent-info, expanded-parent and sibling-info defaults
when it opens: parents appear as a count or indented rows, and aliases display
their ideal sibling. Inherited rows keep their parent's namespace colour and
activate the originating stored tag. Cancelling either Options or Manage Tags
preserves saved preferences and mappings; reopening uses the saved defaults.
The real Qt `tag_dialog_preferences.json` recording covers all sixteen flag
combinations and inherited-row activation.

Options > downloading now includes Default download source. Its button resolves
saved downloaders by key and then name, shows renamed entries, and preserves a
missing-entry caption. The owned gallery chooser lists functional displayed
sources alphabetically, then separate other-gallery and non-functional groups;
broken entries explain their real URL-class/parser/template error and remain
selectable. Empty clients receive the reference warning. Child OK updates only
the Options draft; Cancel, window close and parent Cancel discard it. Apply saves
the default for newly created gallery pages and the subscription Add chooser.

Options > import options opens a staged manager page containing the reference's
three expandable lists: caller defaults, eligible post/watchable/gallery URL
classes, and favourites/profiles. Rows show the actual custom-container summary;
selection survives sorting and profile renaming. Show stack explains the exact
caller/site precedence, while edit opens the shared import-options editor against
its less-specific parent defaults. Global keeps all eight kinds. Clear, deletion
and the three reset choices use the reference prompts and wait for acceptance.
The simple-mode checkbox controls both these children and ordinary importer
editors after Apply. The help menu provides the recorded tl;dr and import-options
manual. Copy/paste and the staged star menus share the existing container exchange
and overwrite helpers. All manager changes remain in the Options draft until its
outer Apply; Cancel also closes and invalidates child handles. Saved defaults,
URL overrides and profiles are visible on reopening and reach importer defaults.
`import_options_panel.json` records the actual Qt lists, stacks, editor kinds and
fallback sources, clear/reset/name prompts, cancellation and apply isolation.

The media viewer's four closing-focus preferences now act on the page that
launched that viewer. Reselecting switches back to that page; selecting exit
media focuses its thumbnail (or collection), retains an existing multiple
selection, and scrolls to it. A background tab change does not redirect those
actions. A closed undoable source receives selection while hidden and reveals it
on undo; an absent exit file keeps its selection. Advanced activation applies
when either focusing action is requested, and debug activation applies to every
close, independently and in that order. Both use the weak main window's native
focus request. Options wait for Apply, persist and import their reference keys.
The real Qt `viewer_closing_options.json` recording covers all sixteen preference
combinations plus missing media, multiple selection, unowned and closed sources.

Regex string matchers offer the reference favourites menu: manage favourites, the enabled no-copy clipboard instruction, and saved descriptions that copy their phrase without changing the regex input. Opening the popup rereads global favourites, so an existing matcher sees choices accepted by another editor. The retained manager stages edits until Apply, cancels without writes, and closes with its matcher; accepted global changes survive cancelling the enclosing matcher.

The four media-viewer background preferences now independently paint passive
copies of tags, file information, ratings/locations and notes behind the media.
They do not depend on pop-in hover enables or window focus and never take input.
Ratings reuse the hover's rating drawing; inbox/trash icons, sorted local and
remote domain names (including pending/petitioned markers), and displayed URLs
join the top-right copy. Notes start below that copy while enabled and return to
the top when disabled, as in the reference. Apply changes the open viewer,
Cancel retains its saved policy, and legacy keys migrate. The fresh real Qt
`viewer_background_options.json` recording observes actual QPainter calls and
pixel occupancy for all sixteen combinations with popups enabled and disabled.
Native rendered regressions also check that opaque media covers these copies.

GUI Pages now exposes the four new-page chooser domain preferences. Combined
local file domains is offered only when at least two local domains exist; its
visibility and top position are independent choices. Hydrus local file storage
has its own visibility and top position, and takes the first position when both
top choices are enabled. Ordinary domains and repositories use service names in
name order, with trash between the local-domain group and bottom storage entry.
Apply changes the next chooser; Cancel leaves its previous settings intact, and
hidden top choices survive reopening and legacy import. Each chosen domain makes
a search with that exact current location, including combined, trash and storage.
`page_chooser_options.json` records all 48 combinations with one, two and ten
domains, actual Qt number-pad placement and each resulting query context.

Tag Presentation > other rendering now edits the sibling connecting string.
The exact saved text, including empty strings and Unicode, joins raw aliases
and their ideal siblings in Manage Tags and every shared write-autocomplete
consumer. Options changes remain staged until Apply; Cancel preserves existing
labels and reopening reads the saved text. The real Qt `sibling_connector.json`
recording uses actual storage and write-result widgets with unchanged raw tags
and counts.

Subscription file work now retains handled import error classes and applies its
failure budget to exceptions that escape per-file option generation or query-tag
writes. The count spans all queries in one sync and resets on the next sync.
Reaching the configured threshold saves the failing file status, abandons the
remaining sync, and persists the configured other-error delay with the reference
reason. Counted failures wait five seconds; typed DataMissing is excluded. HTTP
errors handled inside ordinary file work, including 500 and 404, keep their seed
result and continue without spending this outer budget, as the reference does.

Options > downloading > subscriptions exposes “If a subscription has this many
failed file imports, stop and continue later”. The reference noneable control
starts at 5 errors, permits 1–1,000,000, and its “no limit” checkbox disables
abandonment. Edits wait for Apply, Cancel retains the saved threshold, and legacy
number/None values migrate. New syncs consume the saved network setting.

The URL-class links editor now has “parser links” and “api/redirect link review” tabs. The review computes direct pairs from source example URLs, skips failed converters/self matches, and displays sortable class names. Valid redirect sources use their target’s parser. The exact “try to fill in gaps based on example urls” button appears when gaps remain. The v688 reference owner leaves those gaps unchanged: its matching helper returns new keys, but its replacement loop iterates existing linked keys. The native owner preserves this recorded behavior. Installed associations continue to drive actual API/redirect parser resolution, and the review never changes them.

GUI Pages also controls confirmation for any page close, navigation history
length, and focus when switching pages. Ordinary close confirmations use the
page name; notebook prompts say whether it is empty or how many pages it holds,
including nested notebooks. Importer veto prompts take priority, denial leaves
the tree intact, and acceptance retains normal undo. Session replacement keeps
its separate importer-veto checks. Pages History shows the newest configured
1–1000 entries (default 100) without deleting older history. The focus preference
uses the actual search, gallery, watcher, simple downloader or URL text input
when switching visible pages; sidebars without one leave focus alone. These
controls are staged until Apply and survive reopening and legacy import.
`page_navigation_options.json` records actual close questions, sidebar focus
requests and history menu outputs; headless regressions check real text focus,
confirmation cancellation, nested-tree undo and retained history.

The media-viewer cursor inactivity option now uses the native window input
observer and an owned timer to hide the actual OS cursor after its timeout.
It defaults to 700ms, supports 100–100000ms or “do not autohide”, and uses the
reference's strict elapsed-time boundary and 100–250ms polling cadence.
Movement restores the cursor and restarts the wait; an accepted hidden drag
pauses it until ordinary movement resumes. Losing focus, hovering popup controls
or opening a menu restores the cursor and restarts the wait. Closing the viewer
stops its timer, and weak native registrations ignore other windows and release
closed owners. Apply/Cancel and legacy migration reach this consumer. The fresh
`viewer_cursor_options.json` recording captures actual Qt cursor shapes and
timer intervals, including an actual QMenu nested execution loop.

The downloading Options page's “Maximum number of subscriptions that can sync
simultaneously” spin accepts 1–100, starting at one. Apply saves the limit for
the subscription daemon; Cancel discards it. The daemon reads committed changes
before admissions: raising the limit makes room for another due subscription,
and lowering it lets current syncs finish before admitting more. Each active
subscription has its own ID, status and cancellation; daemon shutdown cancels
and joins every sync. The existing first-running status/cancel API remains
available for callers that display one job.

URL-class parser links now open an owned per-class chooser. Matching parser
examples appear first, followed by the selectable separator and other parsers;
the installed link is selected. Accepting the separator or cancelling preserves
the link. Clear asks “Clear all the selected linked parsers?” and only changes
the draft on Yes. Apply updates the live downloader resolver; closing the owner
retires its chooser and stale callbacks.

Tag Presentation > selection tags now sets the maximum number of thumbnail
items used to compute tags when nothing is selected. The default is 4,096;
“no limit” and zero are preserved. The search sidebar counts the first sorted
items, then includes every member of each collected item. Selected items bypass
the cap. A capped list shows the reference's “for first N files” caption, with
the chosen tag service when applicable. Apply refreshes the active page, and
background pages refresh when activated; Cancel leaves saved values intact.
The real Qt `unselected_tag_cap.json` recording covers twenty combinations of
limits, sort direction, collections, selection and tag service.

URL-class domain masks have the reference simple/full selector, locked while
multiple domains or regexes are present, and an independent domain tester. The
tester trims input and shows matching and normalized domain results; the
subdomain controls retain disabled values. Full masks use independent fixed-domain and regex queues with owned Add/Edit
entry dialogs and counted Delete confirmations. Selected rows edit in order;
cancelling stops the rest. Regex inputs offer advisory validation, clipboard
shortcuts, and the shared global favourites manager. URL previews now provide separately selectable,
read-only stored, request, API, referral, and next-page outputs. Invalid examples
clear stored/request/API outputs and retain prior referral/next results, matching
the recorded Qt owner transition.

The file sort/collect page's “namespace file sorting” button opens an ordered
scheme editor. Add and Edit use the reference's escaped hyphen syntax and clean
namespace names; normal mode uses display tags, while advanced mode asks for
display, multiple-media, or single-media tags. Editing changes the first selected
scheme, duplicate schemes remain independent, and the arrow/delete controls
preserve queue selection and order. Cancelling either question changes nothing.
Child Apply returns an Options draft; only the outer Apply saves it. Reopening
retains the schemes, and new namespaces reach the page's real sort and collect
controls. The recorded queue is in `oracle/fixtures/namespace_sorts.json`.

Media Playback's “Start animations this % in” uses the actual reference0–100
spinbox and preserves imported raw fractions until Apply. Cancel keeps the raw
value; reopening shows its truncated/clamped percentage. Its owned Options
callback rejects hidden or retired input, and saving merges just this preference.
The accepted integer is divided once: actual typed29 saves0.29 and reopens as28,
matching the recorded Qt floating-point truncation; it is not truncated again on save.
Retained old YAML options remain the fallback when no native preference exists.
Native animated WebP/ugoira readers seek before first publication in the viewer,
archive-delete and duplicate filters. Fresh/cleared widgets start at zero even at
100%; reused widgets use the previous widget metadata count, matching recorded
Qt ordering. A saved edit affects the next media admission while the current
paused frame and seek generation remain intact. Impossible initial indices wait
for explicit seek; close drops their reader ownership. MPV consumes no new seek.
The broader playback and per-filetype policy families remain independently Partial.

Media Playback's “Always Loop Animations” now reaches native animation players
in the media viewer, archive/delete filter and duplicate filter. Unchecking it
respects GIF, APNG and WebP stored play counts; missing GIF counts mean one play,
and zero (including ugoira) means infinite. Native finite animations pause on
the final frame, while mpv-backed animations follow the reference's restart-and-
pause behavior. Slideshow stopping still takes precedence. The top-hover zoom
switch offers the reference's four fit/fit-and-fill choices with optional viewer
centering. Its command is captured when the viewer opens; right-click/keyboard
zoom switching retains its separate normal 100%/fit action. Both options stage
until Apply, persist, and migrate their legacy values.

Tag Presentation > default taglist display type now saves independent defaults
for new page sidebars and new media viewers. Both dropdowns offer the reference's
multiple-media view, single-media view, display-tag and stored-tag choices. Each
new list captures its opening value; Apply does not change an existing list.
The two filtered views use their own display filters, display tags apply siblings
and parents without those filters, and stored tags retain raw mappings and
spelling. Options edits wait for Apply, Cancel preserves the saved defaults, and
legacy integer values migrate. `tag_list_display_types.json` records the actual
Qt choices and both real consumers, including changes after each list opens.

Saved file sorts retain their own full tag context, independently of a page's
search and collect context. Namespace and number-of-tags keys read the chosen
service's current and pending display tags; primary and fallback contexts remain
independent for both files and collections. Older native sorts without this
field keep the all-known-tags default, and legacy imported contexts retain
their service, display service and current/pending flags. Options exposes an owned cog on both the default and secondary sort when
sorting by namespaces or number of tags. Its tag-service submenu groups local
tags, repositories and all known tags, retaining independent check states.
Namespace sorts also offer display, multiple-media and single-media tag views.
These choices remain in the Options draft until Apply; Cancel preserves saved
settings, and reopening retains the chosen context and view.

Shared write-tag result lists now support logical multi-selection in Manage Tags,
both sibling/parent inputs and detached additional-tag/favourite editors. A click
selects without changing tags; Ctrl toggles, Shift adds a reversible range and
Ctrl+Shift removes a reversible range. Expanded parent rows share their child's
selection. Enter or double-click activates the selected batch; unchanged fetches
and decoration changes retain it. The reference steps are recorded in
`oracle/fixtures/write_tag_selection.json`. Staged tags still wait for Apply,
and cancelled owners ignore later selection and activation callbacks.

The command-palette provider model now matches the recorded Qt page-tree,
newest-first history, favourite name/folder and menu-leaf filtering rules. Its
provider queue supports extended selection, movement, confirmed removal and
cancelled re-addition. Workers use immutable snapshots and query identities so
late results cannot revive a closed palette or replace a newer query. Options
now exposes the reference boolean controls, typed-query threshold, noneable
limits and inline provider queue. Queue movement, confirmation, re-addition and
all other preference edits remain in the parent draft until Apply; Cancel and
stale callbacks preserve saved preferences. Ctrl+P opens an owned frameless
palette. Page and history results focus actual pages; favourites restore their
full domain, predicate, sync, sort and collect context; menu/media results invoke
existing native dispatchers. Queries run on one worker per palette and stale
results/callbacks cannot affect a reopened owner. Arrow/Page/Home/End navigation,
mouse activation, Escape and native focus loss are wired. The calculator
evaluates the reference's closed arithmetic language on the worker, including
Python power precedence, signed floor division and modulo, large integer results
and its named math functions. Invalid expressions show no row. Selecting a
calculator result keeps the palette open; the calculator bypasses the page/menu
character threshold.

System viewing-time predicates retain the millisecond field, including when
importing stored Python predicates or reopening recent entries. The labels and
actual matching file hashes are recorded from the real editor and database in
`oracle/fixtures/viewtime_milliseconds.json`: 96 combinations of thresholds,
comparisons and canvas selections. Whole-second predicates keep their existing
stored representation. Queries reproduce the reference's conversion back to
integer milliseconds, including its 1.001-second floating-point boundary.
The reference's separate `< 1 second` shortcut selects only zero viewing time;
`< 1.001 seconds` uses its truncated millisecond threshold instead.

A shared write autocomplete selection can now copy tags, subtags, underscore
variants, counts and deduplicated parents together. Its context menu opens the
selection as an AND or OR search, one search page per tag, or a duplicate-filter
page, and seeds sibling/parent editors with the whole selection. All-tag copy
actions appear when other results remain. Multi-tag menus omit single-tag
favourite actions and relationship lookups, matching the recorded Qt menu.

The write-tag context menu also offers “maintenance > regenerate tag display”.
It asks the recorded experimental warning with “let's go” and “forget it”, then
repairs the selected tags and their connected sibling/parent chains across tag
services and file domains from primary mappings. Cancel changes nothing. Repair
retains primary mappings and relations and leaves unrelated counts alone; it
does not enter tags into the editor's draft. Closing the owner invalidates a
pending repair answer. Native display graphs and repaired counts publish together.

Accepted clipboard tags now preserve the text already being drafted in shared
write inputs. Declining the multiline-tag question resumes the native line
editor's normal paste at its cursor/selection, rather than discarding the
paste. Both sibling/parent sides, Manage Tags and detached editors share this
behavior; cancellation invalidates a pending answer. Qt key events, selected
text replacement and accepted clipboard signals are recorded in
`write_tag_selection.json`. Each input now owns Undo/Redo history: replacement
paste is one action, later typing is separate, and fresh editing invalidates
redo. Undo restores the replaced selection without changing another open
editor's draft. Cursor movement, an intervening Undo/Redo and switching
between Delete and Backspace separate typing commands; consecutive edits in
one direction still undo together. A recorded Unicode replacement restores
the original non-ASCII selection, including a supplementary character.

Login required-cookie Add/Edit now opens the name matcher followed by the value
matcher, using the reference titles and initial rules. Cancel at either stage
leaves the pair intact; accepted pairs are sorted and selected. Independent
matcher objects with identical descriptions remain separate entries.
The step editor embeds its cookie matcher list with direct Add/Edit/Delete,
row activation and independent extended selection, as Qt does. Confirmed bulk
deletion and sequential matcher cancellation replay `login_step_cookies.json`.
Step Cancel closes matcher descendants and discards the draft; accepted step
rules reach the existing script persistence and real login executor.

Write autocomplete keyboard selection now follows the result list: arrows wrap,
Shift and Ctrl+Shift extend or reverse a selection, Home/End select endpoints,
and Page Up/Down include expanded parent rows in the visible page distance.
Ctrl+P/N navigate the results. A focused result list supports Ctrl+A and ordered
Ctrl+C, with deduplicated parents on Ctrl+Shift+C; selected text in the editor
keeps native text copying. Keyboard movement scrolls its last hit into view,
without entering tags or changing the draft. The real Qt keyboard sequence and
clipboard payloads are recorded in `write_tag_selection.json`.

A focused write-autocomplete result list consumes the first Escape to clear its
selection, retaining its draft and owner; a second Escape reaches the editor's
Cancel. Mouse dragging adds a reversible range, or removes one when its initial
Ctrl-click deselected the starting tag. Expanded parent rows share their tag's
range position. The drag surface survives suggestion refreshes, and both Escape
and actual mouse handlers are recorded in `write_tag_selection.json`.

File viewing statistics Options now exposes the two reference media-menu display
styles and the media/preview/Client API canvas ticks. The selected canvases reach
view-count and viewtime searches/sorts and actual thumbnail context-menu lines;
combined style sums multiple viewed canvases in a submenu, while stacked style
keeps separate labels. Empty selection removes viewing labels. Changes wait for
Apply, and Cancel/reopen preserve saved values.

File viewing statistics also exposes the reference archive/delete and duplicate
filter switches and media-viewer minimum/cap duration fields, including “count
every view” and “no limit”. Applying changes reaches the actual media viewer and
both filtering windows. Navigation and final close save one interval; same-file
redraws retain the original start, and cancelling filtering decisions still counts
the time viewed. Tracking reads the live policy when an interval finishes, caps
the time before checking its minimum, permits up to five times a file's duration,
and records both filters as media views. Closed or replaced window callbacks
cannot restart an interval or clear the successor's owner. The real Qt recording
covers480 manager outputs,18 duration widget states and actual displayed canvas
intervals; native integration renders the Options page and covers Apply,
Cancel/reopen, real timed viewer caps, filter navigation and stale callbacks.

The main page now displays its focused local file in an owned still/poster
preview beneath the sidebar. Its separate Preview statistics consume the saved
minimum and maximum fields (5s/60s by default), including “count every view” and
“no limit”. Accepted displayed frames keep the original request timestamp;
loading placeholders and rejected decodes count nothing. Re-selecting the same
focused file keeps one interval. Page changes, clear, hide and accepted client
close finish it, while a declined client close keeps it running. Revealing a
hidden preview splitter waits for a fresh focus change; restoring a page restores
its focused file. Settings are read at finish, with cap-before-minimum and the
duration-times-five extension, and Preview rows persist independently of media
viewer rows. Late frames and callbacks cannot reach a replacement page/request
or rebound owner. A lazy owner pool caps still decoding at two workers and
retains only the latest queued target. A held obsolete decode therefore permits
one successor to display; idle workers retain no Store, and close discards queued
work and retires running replies. Actual Qt Options, manager and CanvasPanel transitions are
recorded in `preview_viewing_intervals.json`; native model/display/store regressions
are authored for hosted CI. Preview playback, audio, zoom and hover/rating controls
remain Partial.

Search-page autocomplete now shares results, favourites and children tabs in
both its embedded and floating layouts. Favourites show the full saved list;
children show count-ranked descendants of top-level search tags, exclude tags
already in that context, retain zero-count descendants, and honor the shared
finite or unlimited cap without printing counts. Selecting a favourite or child
enters its predicate in the actual search; removing a predicate updates children.
Typing returns to results, while choosing a tab preserves the draft. Options
Cancel/Apply and restored-page consumers are covered against
`oracle/fixtures/read_tag_tabs.json`. Empty tabs retain the tab selector.
Accepted favourite-menu edits publish a settings revision to other live read
and detached write panes, preserving each owner's input. Locked read pages
consume the pending revision after unlocking; cancelled detached editors stop
their subscription.

Downloader HTTP requests now perform domain login admission before acquiring a
connection slot. Active most-specific login domains use current credentials and
shared cookies; inactive/already logged-in domains proceed directly. One
store-owned file lease serves queued requests across independently opened GUI and
daemon engines and real manual/forced login attempts. Login-step requests bypass this admission. Cancelling
a triggering download preserves the login for other requests. The reopened domain
manager monitors the persisted live login owner directly, without needing daemon
snapshot publication, and can cancel that process by its reviewed owner identifier; this cancellation persists the reference login delay.
Ordinary jobs show invalid/delayed-login status and wait; subscription jobs cancel
with the reference explanatory note.

Login admission uses the existing crash-safe store file-lock mechanism. Owned
engine clones retain their current control, while owner epochs and cancellation
are persisted per store. Dropped/unpolled processes retire only their owner;
stale metadata after a crash cannot keep the admission lock or cancel a new
process. Queued manual cancellation preserves the current global login, and
forced login still executes with existing session cookies.

Files and Trash now stages four independent removal preferences, all defaulting
to false. Archive/delete filtering removes committed keep/delete rows only when
enabled; its dependent “even skipped files” checkbox preserves its saved value
while disabled. With skipped removal off, the first surviving skipped file is
selected in the source page. Forget/resume leave rows untouched. Filters retire
predecessors and reject hidden, closed-source or retained retired commits; a tab
switch preserves their original page rather than removing from the new page.

Trash removal uses actual trash membership after successful thumbnail/viewer
deletions. Deleting from one of several local domains keeps the row while another
domain owns it; a one-domain trash view keeps newly trashed rows. Physical
removal still prunes wholly current local views independently of this option.
Successful thumbnail strict/merge moves remove only actually transferred rows
from a view including their source domain, when enabled; copying keeps rows.
Options Apply affects these live consumers, Cancel discards staged edits, and
legacy preferences migrate. `files_view_removal.json` and its Qt PNG record real
controls, filter close signals and media-list content updates. General external
content-update broadcasting and broader Files and Trash behavior remain Partial.

Files and Trash now saves the confirmation preferences for trash and multi-file
archive/inbox operations. Thumbnail shortcuts and menu actions read the applied
preferences; the viewer also skips a simple local deletion question when trash
confirmation is disabled. Multiple actionable local domains and physical deletion
continue to ask. Cancelling Options leaves these preferences unchanged.
The real Qt inputs and questions are recorded in `files_trash.json`.

The advanced Files and Trash deletion dialog now lists actionable local domains,
all-local trashing, unlocked physical deletion and physical deletion with cleared
records. Its reason radio rows preserve existing reasons, offer the ordered
custom suggestions and accept custom text. Accepted action/reason preferences
are recalled when applicable. Options embeds the ordered reason queue with
Add/Edit text children, movement and confirmed removal; all queue edits remain
staged until Options Apply. Closing the owning viewer discards its open deletion
draft and invalidates retained callbacks. `files_trash.json` records the Qt flow.

Read autocomplete now constructs an OR with Shift+Enter without changing the
active search. Its sorted, deduplicated draft appears first in results; ordinary
activation of another term commits the completed OR, while choosing a one-term
draft commits that term alone. Rewind and empty-input Escape remove the final
sorted term, then cancel the remaining draft. Cancel clears the input and leaves
the active search alone. Committed OR predicates query real files and survive
favourite-search persistence; draft state is never saved with a search. The
actual Qt state and query count are recorded in `read_or.json`.

The read autocomplete OR button opens an empty staged search editor in the
caller's file/tag context. It uses the same results/favourites/children lists,
system-predicate child opener and nested OR construction. Accepting zero terms
clears the caller input, one term unwraps it, and multiple terms become an OR.
Advanced mode exposes the advanced input dialog with a live result preview. It
accepts not/and/or/implies/xor/xnor/nand/nor and their symbolic forms, parentheses
and escaped tag text, distributing clauses into AND-of-OR search predicates.
Invalid input stays open with the recorded validation message. Both children
leave caller text and predicates intact on Cancel; a closed caller, cancelled
ancestor or retained old child cannot apply its staged search. Their recorded
Qt dialogs, parser outputs and actual query counts are in `read_or_editors.json`.

System predicate editors opened from the main read input or basic OR child
retain the selecting Shift key: accepted values extend an OR draft with Shift, or merge and commit
it with normal activation. Cancelling the system editor leaves the draft and
active search alone. `oracle/fixtures/system_or_activation.json` records 18
actual Qt cases in the main read input and basic OR child, including accepted
system recents surviving outer Cancel. Both callers use the original activation intent. Authored model replay and
the native 18-case consumer replay await hosted CI; no native render is claimed.

The file-size editor displays `<`, `≈`, `=`, `≠` and `>` as five radio choices,
with bounded arrow navigation, Space and default Enter acceptance. Its amount
stays within 0–1,048,576; selecting B/KB/MB/GB/TB changes the binary multiplier
without converting the amount. Typed values and saved custom defaults retain
the selected unit, including zero KB and unnormalised 1,024 B. Cancel leaves the
search unchanged. Hidden owners or a pending main question refuse edits/acceptance; rebinding the main
window or accepting client exit cancels its owned predicate child. A declined
exit leaves it live. `filesize_predicate.json` records actual Qt controls,
30 database-query outcomes, numeric bounds, explicit-value reopening, keys and
Cancel. Authored model/native replays and the native screenshot await hosted
execution; the actual Qt screenshot has been inspected.

Manage Tags now reads deleted mappings separately for each selected local tag
service. The count measures tag–file mappings, hides at zero, and follows staged
deletions and re-additions. Show/hide is an immediately saved global preference
that survives Cancel and reaches other open owners. Rows combine current `(n)`
and deleted `(Xn)` counts, including current counts when every selected file has
the tag. Apply commits the private mapping draft; Cancel and callbacks retained
after closure cannot change it. The real Qt recording
`manage_tag_counts_incremental.json` covers two local services and reopening.

Manage Tags' ± Incremental Tagging button now opens an owned child for multi-file
selections. Namespace, prefix and suffix remember each edit immediately, including
Cancel; start defaults to the first file's first decimal subtag in the reference's numeric ordering,
step defaults to one, and reverse numbers the original selection backwards.
The preview reports the exact tag sequence and existing namespace conflicts.
Child Apply adds one tag per original file to the selected service's private
Manage Tags draft, preserving other namespace tags. Parent Apply commits it;
child/parent Cancel and window closure discard pending mappings and invalidate
retained callbacks. While the child is open, service changes, tag entry and
parent Apply are blocked. Reopening an existing Manage Tags owner preserves its
original selection and file-count title. Actual Qt recorded child/parent cancellation, negative
steps, reverse order, remembered reopening and persisted per-file mappings.
Additional Qt recordings cover initial-value and numeric-control clamp boundaries,
long ASCII and Unicode leading-zero inputs, and reference initialization failures.
Native safely clamps large initial values and accepts long decimal previews.


Options > GUI now shows the reference's complete frame-locations table, including
imported unknown frame names. Its edit child stages remember-size/position,
optional size/coordinates, default gravity/position and maximised/fullscreen.
Batch flip/reset and sorting retain selection. Child Apply changes the Options
draft; Options Cancel discards it and closes/inactivates the child. Main-window
and media-viewer remembered geometry/state use their existing placement consumers;
the Options window also places itself from its `manage_options_dialog` frame.
Its explicit frame-table edits persist on Apply; Cancel/X preserve the previous
geometry. An accepted reset of its own size/position remains unset, matching the
actual reference dialog's save-before-frame-table-commit order. Live main/viewer
geometry saves merge inside a writer transaction, preserving other frame keys.

Local service review bulk maintenance uses the exact questions and “do it” /
“forget it” decisions from `oracle/fixtures/service_bulk.json`. Trash controls
disable when empty. Undelete restores every former local domain and its import
time; clear uses the normal physical-deletion queue and honours archived-file
delete locks. Like/dislike, numerical and inc/dec rating services offer “for
deleted files”, “for all non-local files”, and “for all files”. The first two
use physical storage membership, so a file still in trash remains local.
Counts refresh on acceptance and survive reopening. Each question captures its
service/action, blocks replacement and selection, and retires with its owner.


Options > tag presentation now opens a detached editor for each thumbnail and
media-viewer tag banner. It edits enabled state, RGBA colours, separators, ordered
namespace prefixes and separators, and cleaned example tags with live preview.
Child Apply stages the generator; parent Apply saves all three and refreshes
thumbnail banners and the title of an already-open viewer. Cancelling either
owner discards its draft and invalidates retained child callbacks. The Qt oracle
drives all three reference buttons, namespace CRUD/movement and live preview.

The banner editor child is held explicitly by the main window's Options owner.
Its slot clears on child or parent close; retained stale callbacks cannot clear
a replacement slot or commit an old draft. No test-only global window registry
is used for banner editing.

Physical-storage service review offers “clear deleted files record” with both
reference confirmations and their distinct yes/no labels. The first acceptance
only opens the second question; either decline or owner close leaves the store
unchanged. Final acceptance uses the existing transactional record-clear path:
files still in trash retain their history/reasons, other local deletion records
are forgotten, and pending physical deletes remain queued. Reopened review counts
and the import-status consumer reflect the change. The actual Qt decisions and
status transition from deleted to unknown are in `service_deleted.json`.
Opening another service review cancels the previous owner's pending question;
retained callbacks from that retired owner cannot clear records or close the
replacement review.

Numerical rating examples keep their opening allow-zero setting even when the
live star count drops to one. The displayed sample remains “1/1” for an opening
nonzero scale; saving the one-star service still forces allow-zero, as Qt does.
`rating_preview_one_star.json` records all four samples through that transition
and back to seven stars, including checkbox changes that affect only the saved
configuration.

Local rating service edits include an expanded “example display” with independent
Thumbnails, Media Viewer, Preview Window and Dialog (Default) samples. Colours,
shape, numerical star count, spacing and fraction placement repaint the draft.
Like/dislike samples toggle; numerical right-click clears; counter clicks add or
subtract, with middle-click opening an owned value prompt. Sample values are
never saved as file ratings. Child Apply stages only the service configuration;
parent Apply persists it. Cancel, reopening and closed-owner callbacks leave
samples and unapplied configuration behind. The actual three-kind Qt replay is
`service_rating_preview.json`; hosted native coverage also saves a populated PNG.


The browser viewer's eye button now opens view options. The three Media Viewer
Hovers collapse controls stage independently in Options, save on Apply, and
reopen with their saved values. A checked control puts window, hovers or rendering
rows in a submenu; an unchecked control puts that section's rows directly in the
menu. Each opening reads current preferences, including in an already-open
viewer. Actions reach native always-on-top/frame properties, new-viewer defaults,
existing background/pop-in hover consumers and checkerboard/greenscreen drawing.
Menu callbacks belong to their viewer slot and retire on close. The eight actual
Qt combinations are recorded in `oracle/fixtures/viewer_eye_menu.json`; authored
model/native/store regressions cover staging, reopening and real consumers.

Numerical rating examples accept left-button dragging and clicks on either
fraction label through the same whole-widget control as their star graphics.
Dragging outside keeps the last valid sample; an outside press clears it. Hover
motion after release does not change a rating, and each context remains independent.
The pointer-route Qt recording is `rating_preview_pointer.json`.


GUI Pages now stages notebook tab alignment (top, left, right or bottom),
width-based middle elision, and the experimental hide-navigation-tabs setting.
Apply persists the choices and refreshes the existing nested notebooks without
changing their pages or selection; Cancel leaves the open layout unchanged.
Left/right labels rotate in their corresponding direction, and nested rows sit
inside their parent row on each side. Their labels are rasterized with the native
font resolver so the software renderer paints the rotation too. Full names remain
available as hover text.
Tabs keep their full stored labels when space is scarce: enabled elision measures
native glyphs and replaces the middle only for painting; disabled elision offers
small scrolling arrows without reducing the label height.

The experimental hide flag takes effect only while the tree is enabled, matching
Qt's gate. Its supporting native hierarchy selector lists every existing page on
the chosen left/right side and selects by stable page key, including remembered
children of notebooks. The collapsible tree behavior is described below; broader
Qt tree drag/drop and context-menu behavior remain unported. Actual Qt choices,
bar positions, hidden states, selected indices and fitted strings are recorded in
`tab_presentation.json`, alongside Qt orientation PNGs. Native consumer replays
capture each side, hidden navigation and overflowing labels for hosted review.

Options → ratings now stages independent Preview Window and Dialog square-icon
sizes and counter heights. The four controls retain Qt's separate bounds, two
decimal places and clamping, including Qt’s 31.755 → 31.75 rounding boundary;
Apply persists them, Cancel discards them, and
legacy import preserves the saved fractions. Newly opened Manage Ratings uses
the Dialog preferences for like/numerical graphics, their outlines and counter
height, with counter width expanding for long values. Held right-button movement
clears without rating stars; held left movement rates the dialog's draft. All
four service-editor examples read their corresponding saved sizing preferences.
The still/poster preview has no rating controls consuming Preview Window sizing yet.

Numerical rating examples also continue a held Left drag across a Right
press/release while the pointer remains in the sample; a right-only drag does not
set a rating. The actual Qt chord sequence extends `rating_preview_pointer.json`.
The cross-edge chord capture boundary is documented separately in DIFFERENCES.

Database > file maintenance > fix missing file archived times runs a global
scan, with the reference's initial warning and separate legacy/import/both
choices. The owned window shows scan and repair work, supports cancellation and
reports completion or no missing times. Current local media, trash and former
local deletion records use their recorded import/deletion times. Accepted
repairs go through the content writer and refresh the main page's actual media
results; an intervening archive/inbox/time edit is preserved. Closing, replacing
or invalidating the owner prevents a retained confirmation from starting work.


Options > tag suggestions now stages the suggested-column width, notebook or
side-by-side layout, the original four default-page choices, and independent
most-used tag lists for each real tag service. The owned list editor reuses
write autocomplete, paste and selected-tag removal; its child Apply accepts a
list, its own Apply accepts the service drafts, and Options Apply persists them.
Cancel at any level discards that level and closes its descendants. Saved
per-service edits merge without replacing concurrently changed other services
or the children-tab cap.

Manage Tags shows functional most-used and recent suggestion lists. Most-used
rows follow the search-page tag sort and omit current/pending tags already on
every selected file. Activating a selected batch only adds missing mappings;
these additions remain staged until Manage Tags Apply. Existing most-used
panels refresh immediate shared-menu/Options changes within 200 ms. Recent rows
read the imported/native history, and accepted tag additions update that history.
Opening captures width, layout, panel availability and default tab; reopening
uses saved settings. The actual Qt replay is `tag_suggestions.json`, including
two service drafts, removed tags, cancellation, all eight recorded available
layout/default combinations and add-only activation.

The experimental GUI Pages tree-view option now shows a collapsible hierarchy on
its chosen side. Disclosure arrows and Collapse all/Expand all preserve stable
page identities and descendants' expansion state. Single click and Up/Down,
Left/Right, Home/End move a separate tree cursor; Return or double-click activates
that notebook/page. Switching ordinary tabs reveals the active page's ancestors,
and changes to the session retain the cursor by page key. The hierarchy remains
available when main navigation tabs are hidden. Options Apply/Cancel, saved
settings and reopening use the existing staged tab-presentation controls.

Database > view file history opens an independent local-domain chart. It reads
current/deleted imports, deletions, inbox and archive memories to draw the four
reference sampled series. Series toggles, count/date ranges and both refit
buttons affect the plot; refresh preserves custom ranges. Typed tags and system
predicates run actual independent current/deleted queries. Cancelling, refreshing
or closing prevents an older background result from replacing the current chart.
The parent page's predicates and session tree are unaffected.

Each history window owns one worker. New queries cancel earlier work and replace
one pending request; rapid refreshes do not start more threads. Closing drops
the request channel and pending/result slots without joining on the UI thread.
A worker startup failure leaves the window usable, displays the error and allows
Refresh to retry.

The duplicate filter's Options page includes the separate A/B background
light/dark intensities (1–9 or “do not change”) and its independent transparency
checkerboard preference. They remain drafts until Apply; Cancel retains saved
zero values, while Apply saves the spin box's displayed minimum of one, as Qt
does. Existing duplicate-filter canvases, including rule previews/reviews, read
the saved preferences live. Pair switching applies A/B to the original ordered
pair, and transparent media show the reference's 16 px checkerboard or shared
greenscreen brush only within their image box. Imported active override canvas
colours are preserved. `duplicate_colours.json` records actual Qt controls,
143 QColor adjustment cases and eight A/B painter outcomes; model, Options
owner and native filter painter regressions replay those outputs.
GUI Pages navigation now stages the six drag/wheel preference checkboxes with
independent ordinary/Shift choices. Pressing and holding a notebook tab starts
an owned drag after the reference's 100ms threshold. Moving over another tab can
navigate to it; releasing reorders or transfers the original page, including
between nested notebooks. The normal and Shift chase choices independently
select the moved page or retain the reference's source-notebook selection.
Escape cancels, and disabling page-tab drag/drop prevents the gesture. Pointer
capture survives navigation that replaces the visible nested tab rows; release
uses live geometry and stable page keys, preserving media order and selection.
Rebuilt tab rows publish each initial laid-out rectangle with its page identity,
so repeated pointer drags continue to hit the moved page after a reorder.
The wheel preference reaches the actual
notebook bars: default wheel input selects the adjacent tab and clamps at either
end; enabled scrolling moves the overflowing bar's viewport and preserves page
selection. This works on horizontal and vertical notebook rows; Apply refreshes
the live bars and Cancel leaves saved settings unchanged.
Options > tag suggestions opens an owned related-weight draft with separate search/suggested namespace tables. Click either header to sort by casefolded namespace or numeric weight, with the full row breaking ties. Each table retains its own selection and sort direction; Add selects only the new row. Reserved and duplicate namespace warnings, 0–10,000 percent values, protected catch-all rows, add/edit/delete, and question Cancel follow the recorded Qt editor. Child Apply stages the tables; parent Apply persists them, while parent Cancel closes and invalidates the child. Already-open Manage Tags related lists re-query saved weight changes through one owned worker. Related suggestions preserve score order, filter tags already present on all captured files, and only add mappings when activated.

Read autocomplete favourites and children now share the write list's reversible
Ctrl/Shift selection. Enter or double-click broadcasts the selected batch;
Shift sends it to the current OR draft, and cancelling that draft keeps the
active search unchanged. Children exclude the tags just committed and keep the
count-ranked, countless descendant list. Read panes expose the same immediate
favourite/most-used add and confirmed removal actions as write panes. Visible
read and detached write owners refresh changed favourites/children limits
without clearing input drafts. A hidden, replaced, locked or switched read owner
cannot accept a delayed favourite answer. The shared favourite editor remains
an Options draft: child Apply stages it, parent Cancel discards it. Favourite and
child tags containing wildcard or system-looking text remain literal inclusive
tag predicates, preserving already-stored values such as `system:inbox` without
reapplying entry cleaning. Children lookup separately follows Qt's database
cleaning boundary: an active literal `system:inbox` uses the `inbox` parent
chain, while a wildcard-containing parent retains its literal spelling.
`read_tag_tabs.json` records activation, child exclusion/removal and restored
negative-tag contexts across that boundary. Enter with no selected rows leaves
the active query and OR draft unchanged. Locked read panes defer external
settings refresh until unlocked.
Options > thumbnails > interaction now stages the Shift-selection keyboard
origin, visible-percentage threshold and scroll-rate text. Apply updates the live
grid; Cancel preserves saved settings and reopening uses them. The origin choice
keeps preview focus separate from the last hit, so a plain movement key can start
from the most recent Shift-selected file without changing the selection rules.
Key navigation uses the saved strict visibility threshold, Qt's reveal margin
and the content-end clamp. Actual thumbnail wheel events use the saved rate,
including ties-to-even rounding, zero and negative rates; malformed text retains
the previous setting. The reference replay is `thumbnail_navigation.json`.
The four independent Ctrl/Shift preview-focus checkboxes are also staged here.
The no-duration child is enabled by its own parent and preserves its checked value
while disabled. Shift focus on all files disables the existing ghost-navigation
control without changing its saved value. Modifier additions and anchored ranges
optionally focus their target, including Shift keyboard navigation. Ctrl removal
clears only that item's current focus; plain clicks and anchorless Shift fallback
focus normally. The duration restriction examines the clicked item (all members
for a collection), keeping last hit and the range anchor independent from preview
focus. The existing preview canvas supplies owned pixels and viewing intervals;
hidden main-window callbacks leave selection/focus unchanged; re-show stays live.
An accepted main-window close permanently retires these selection callbacks, while
Cancel keeps them live. `thumbnail_preview_selection.json` records actual Qt
checkbox dependencies, save/reopen, ranges, focus publications and duration shapes.
The search sidebar’s sort cog appears for namespace and number-of-tags sorts.
Its tag-service submenu groups local tags, repositories and all known tags;
namespace sorts also offer display, multiple-media and single-media tag views.
Each choice immediately sorts the actual page using its independent context.
The collect cog changes its own service or unmatched-files policy and immediately
regroups the page. Every opening refreshes checks; retained actions cannot change
a different page, a changed control or a hidden owner. The Default collect cog in
Options uses the same service choices, staged until Apply, preserved after
reopening, and discarded by Cancel. Both controls preserve the full tag-context
metadata independently from the page’s search context.

Options > tag presentation now stages namespace-colour additions and confirmed
multi-row deletion. Add uses the reference's namespace cleanup and exact warnings;
default namespaced and unnamespaced colours are protected from deletion. New
namespaces receive random RGB colours. Rejected input opens an owned Warning
notice with the exact message and an OK acknowledgement. Enter Text, confirmation
and warning children block parent Apply, and Cancel discards the Options draft. Accepted colours
persist across reopening and reach existing media tag and OR predicate rows.
The OR-row namespace text preserves named, missing and empty namespace choices;
explicitly entering empty text selects the unnamespaced colour even when the
saved legacy value was unset.
Namespace colour Add preserves the reference list’s positional Shift-range bookkeeping after sorting, while selected namespaces follow their rows. Nested namespaces retain their literal trailing-colon label, as Qt paints them. Delete stays available for empty or protected selections and quietly leaves those rows alone. Namespace input also accepts the reference’s control-character whitespace trimming before reserved-name and duplicate checks.
The sibling connector namespace editor reserves room for the full “use ideal tag
colour” checkbox caption; its text input shrinks and scrolls within the Options row.

Options > GUI > frame locations also stages the disable-rescue checkbox,
rescued-position safety padding and its 0–100 pixel amount. Legacy preferences
import and reopen with their recorded values. Remembered positions for the main
window and existing named dialog owners now reach an owner-local opening check
when their Winit window becomes available. It preserves a top left within the
configured leniency, otherwise tries the three remaining window corners in the
reference order and falls back to the primary display. The disable checkbox
preserves the requested position. Hidden owners defer the one-time check.
`window_rescue.json` records actual Qt Options staging/bounds/serialization/Cancel,
real offscreen display decisions and the unmodified Qt rescue handler over an
explicit two-display topology. Native geometry and Options screenshot regressions
are authored for hosted CI; native OS monitor behavior has not been executed here.

The main opening observer composes with batched file drops in one window-owned
Winit filter. It reads the native client size before Slint updates its cached
event geometry, defers zero-sized openings, and retains the one-shot decision.
The headless regression verifies the exact rescue result and drop forwarding;
its adapter cannot verify OS window positioning.

Options > open externally now stages an ordered registered URL-call queue and
single-file routes by filetype. Add/Edit show the registered-call chooser even
with one remaining choice, excluding every identity already in that queue.
Filetype Add keeps the reference general-class and searchable-type order and
opens an owned calls child; Edit keeps the existing order, and Cancel discards
that child. Queue arrows change the first/default call; removal captures the
selection and asks first. Any selection containing “all files” protects all
selected MIME rows from deletion. Parent Apply alone saves routes, washes removed
or wrong-type keys and updates renamed labels; Cancel discards accepted children.
The opened routing draft supplies registered OS defaults for empty queues.
Options page/search navigation and child launch paths keep shortcut and routing
children mutually exclusive until the active child closes or the parent cancels.

The main thumbnail and viewer default-open actions read current saved routes.
Specific filetype entries replace general-class entries, then “all files”;
an explicit empty specific entry uses the OS default. The first URL/file call
receives typed URL or path/URI/hash/file-id inputs through the existing process
pipeline. A configured missing or wrong-type call shows an owned Information/OK
notice instead of silently trying another call. OS defaults use the existing
native launcher. Dispatch reads live file-domain membership separately from basic
hash/file metadata, so locally restored files are eligible immediately and deleted
files remain ineligible even when their bytes remain. Accepted client close, main-window rebind and viewer close
permanently retire their launchers and notice/completion callbacks;
submitted external programs retain their own lifetime. Options children reject
hidden and retired owners. Deeper per-call media/URL menus, routing import,
column/keyboard persistence and full process output/error presentation remain
Partial. `open_externally.json` and its Qt PNG record the list/chooser behavior;
authored native replays await hosted CI.

Options > external programs now has a staged registered-call table with name,
job and command columns. Add/Edit own a detached callable editor and command
queue; parent Cancel discards accepted child drafts, and parent Apply persists
registered calls. Delete uses the reference's ordinary captured-selection
question, including OS-launch entries. Duplicate generates fresh keys and
nonduplicate names, retains prior selection and asks about unusually large
commands; declining a later warning preserves earlier unselected, appended
copies and aborts the remainder. Add Defaults exposes both “add them all” and
“select from a list”, with the reference platform question and factory calls.
Selected defaults gain fresh keys and names alongside the prior selection.

The owned command child edits the first parameter in selection insertion order, keeps
added rows unselected, captures delete selections, and reorders adjacent selected
rows at either boundary. Its focused parameter list preserves range origins through select-all, Ctrl
navigation and deletion, and supports extended selection,
arrow/Home/End navigation, select-all, selected-text copy and Delete (also
Backspace on macOS). Preview and full-template copy use cleaned parameters while
pasted raw rows remain visible until Apply. Paste asks the exact unsorted summary
question, including quoted, empty, repeated-space and Unicode text; declining it
keeps the draft. Clipboard failures use an owned Error/ok notice. Copy and accepted
paste show the reference's brief feedback. Parent Cancel and retired children
cannot publish drafts; accepted arguments persist and feed actual token expansion.

Supported process calls expose enabled input rules, tokens and the existing
string-processor child, an ordered command-argument editor, timeout/flags and
preview/test inputs. Reopening a saved process and pressing its Test Call runs
its argument vector in an owned worker. Closing the owner cancels and reaps its
direct child; output is discarded, and long-lived test calls use a 15 second
deadline. Clipboard/JSON-file/PNG exchange reviews supported callable exports
before changing the Options draft. These editor/exchange/runtime families remain
partial: legacy executable-manager import and missing OS-call regeneration,
deeper per-call launch menus and routing import, OS-launch test execution, rule clipboard controls,
full process output/error handling and some command/dialog interactions are not
ported. The complete scope is recorded in the external-call parity proposal.
Options > media viewer > mouse behaviour now stages cursor anchoring and its
apparent-touchscreen override. Apply updates an already-open viewer; Cancel
discards the draft. Anchored drags pan about the press position and request a
cursor warp back through that viewer's native window. The optional override
unanchors after a single movement exceeds 50 pixels of Manhattan distance and
stays unanchored until the next press. Both preferences survive import and
reopening. The actual Qt replay is `viewer_anchor_options.json`.

Options > media viewer hovers now stages all four taglist wheel propagation
policies. The raised tag hover is a clipped, scrollable list with a draggable
scrollbar. Wheel input first scrolls the list; an edge then either stays in the
list, propagates only without a scrollbar, waits for a quiet same-direction
wheel, or immediately reaches the viewer's actual next/previous/zoom action.
The delayed policy preserves Qt's 0.57-second media grace and direction-change
gate. Apply refreshes an existing viewer, Cancel preserves its current policy,
and the imported setting survives reopening. Existing scroll position survives
a media change when it still fits, and clamps when the new list is shorter.

Options > GUI > misc stages the ISO-time checkbox and bytes precision spinbox
(1–6), imports their exact legacy keys, and preserves Apply/Cancel and reopening.
Owned formatting reaches existing log, page/status, file-info, service/import,
network review/rule/job/graph, import-options, parser-preview and PNG labels;
live download and popup progress preserve gauges and cancellation ownership.
ISO also reaches About, cookies, subscriptions, downloader timestamps and
viewing-statistics menus. Python applies the current local offset to every date;
the native formatter does the same, with recorded opposite-season timezone and
POSIX year-one cases. Manage Times and duplicate-review relative suffixes stay
relative because the reference explicitly forces that behavior.

`gui_format.json` records eight actual Qt staged/saved/reopened control states,
byte/time/log outputs, import summaries, raw parsing previews, expiry and
relative-time variants. Five owned status inputs per setting pass through the
actual Qt network-job control; no request is sent. Native consumer replays and
an Options PNG capture are authored for hosted CI, with no local Rust builds.
`gui_format_backend.json` additionally records real backend bandwidth/gallery
waits, file-size rejection, over-length responses and critical-drive messages
across four staged/saved/reopened settings. The network engine owns saved
formatting and refreshes waiting labels without replacing job state, bandwidth
usage or gallery deadlines. File importers read saved precision when rejecting
a file or reporting a nearly full drive; their pause behavior is preserved.
Focused asynchronous, loopback HTTP and real file-import regressions are
authored for hosted CI. Whole-application cached-label broadcast timing is
unclaimed. The radio Return flag is unclaimed because the reference dialog
behaved identically in both states on the recorded platform. Broader
GUI/misc/menu stays Partial.
Tag Presentation offers the sibling connector colour controls. Fading disables the optional namespace choice and uses the ideal tag’s colour; turning fading off allows a named, missing, empty, or ideal-tag namespace choice. Apply saves the staged preferences and Cancel discards them. Manage Tags paints raw/count, connector and ideal-tag runs separately with solid colours; shared write-autocomplete rows can fade between namespace colours, including selected backgrounds. Detached tag editors and both relationship inputs share those runs, and open, visible editors refresh after another owner changes the preferences without replacing the input or selected tags.

Selected sibling rows paint solid namespace runs from each run’s left edge through the remaining row, then overlay each fading run only within its fixed text block. Explicit background positions keep later solid runs from covering an earlier connector fade. A collapsed-parent suffix can fade to the unnamespaced colour while the preceding ideal tag’s solid colour remains beyond the suffix, including when the viewport widens.


Options > maintenance and processing includes separate browsing, mouse movement
and Client API idle timeouts under “when to run high cpu jobs > idle”. Each accepts
1–1000 minutes or its own ignore checkbox. Defaults are 30 minutes for browsing,
10 for mouse movement and ignored API activity; imported values retain their
stored seconds and the editor displays whole minutes as Qt does. Unchanged Apply
saves those displayed minute bounds, while Cancel preserves the original seconds.
Implicit normalization preserves a newer value written while Options was open.
Apply updates
the existing running idle-only session autosave gate; Cancel keeps the saved
thresholds. Reopening ignored controls restores the reference's hidden one-minute
value. The controls are disabled when the stored idle-enable flag is off. Mouse
activity currently covers native application windows, while browsing and Client
API activity use the existing all-window and shared API timestamp consumers.

Options > importing now stages all five work-slot limits (1–500): gallery file
queues, gallery searches, watcher file queues, watcher checks and other paged
importer jobs. Apply reaches already-running daemon queues; Cancel preserves
saved limits, and reopening/import retains them. The actual loaded reference
defaults are 15/5/15/5/10, replacing its spinner constructor seeds of
5/15/5/15/10. Imported raw integers remain intact until Apply normalizes the
shown controls. `import_work_slots.json` and the inspected Qt PNG record the
controls and the real controller's named acquire/release boundaries.

Each runner owns independent active counters and cancellation-safe permits.
Lowering a capacity preserves running counts and blocks new work until enough
permits release; raising it admits pending work. Bandwidth/network waits still
consume a work slot. Watcher checks and files now run independently, so a
checker waiting for its own slot does not prevent that watcher's file imports.
URL and simple-downloader file/page work and local-import files share the other
paged-job pool. These limits are separate from network connection semaphores.

Database > clear now opens the original clear-all and cull-viewing-statistics
questions with “do it”/“forget it” choices and the original completion notices.
Clear removes every canvas's view counts, durations and last-viewed records.
Cull reads the current saved media and preview rules when accepted, validates
both before writing, reduces excessive counts before limiting total durations,
and preserves other canvases and timestamps. Preview minimum/maximum rules are
editable and migrate from the reference; older native settings use the reference
5-second/60-second defaults. Decline, closing the owning window and callbacks
from retired questions preserve the records and any successor question.

An importer's per-service tags page now offers “set a filter for already-exist
test”. It opens the shared whitelist/blacklist/advanced tag-filter editor with
the reference explanation and parser namespaces. Accepting the filter enables
“only add tags that already exist” for that captured service and changes only
the importer draft. Cancelling the child preserves its filter and toggle;
cancelling or closing the importer closes the child and invalidates retained
callbacks. Parent Apply waits for the child. Saved importer options retain the
filter after reopening and feed the file importer's existing-tag consumer for
both parsed and additional tags. Only tags admitted by this filter are tested
against current mappings on the chosen service; excluded tags bypass the test.

Files and Trash now exposes the reference's independent copy and move confirmation
checkboxes. Both default to enabled; imported values survive reopening. Edits stay
in the Options draft until Apply, and Cancel or a retired Options callback cannot
save them. Saving one checkbox preserves a concurrent change to the other.

The thumbnail menu's local `locations` actions read those saved preferences when
invoked. `add to`, `move (strict)` and `move (merge)` capture their files and explicit
source/destination. Enabled gates ask the recorded Add/Move/Move-merge question;
yes applies, while no, Escape, window close, a hidden question or a retired page
cannot write. Switching away retires the question even if the page is revisited.
Disabled gates perform the same transfer immediately. The transaction checks live
local memberships and stable service keys, restores a deleted destination's original
import time, adds the destination before removing the source, and keeps physical
storage and inbox state. `local_transfer_confirmations.json` records actual Qt
Options, the local action menu and real migration writes on a copied fixture DB;
model/native regressions cover those decisions, persistence and stale owners.

Options > shortcuts stages the reference numpad-merge and primary/secondary
mouse-label policies and opens an owned shortcut-set editor. Each command child
keeps independent keyboard and mouse captures. Keyboard capture retains raw
native key location before Slint conversion, records sorted modifier identities,
casefolds characters, and applies the non-number numpad policy. Child OK updates
the set draft; set OK updates Options; only Options Apply persists. Cancel and
closed owners reject retained child callbacks. Saved main GUI bindings call the
existing refresh, close-page and new-page callbacks; saved media viewer bindings
call close, navigation, zoom and fullscreen callbacks. This slice offers only
those commands and the two reserved sets, leaving broader set/command management
Partial. Existing hardcoded shortcuts remain fallbacks for unmatched bindings.

Mouse capture includes press/release, double-click and vertical wheel gestures,
with the recorded strict small-wheel accumulation and disabled release selector
for wheel/double gestures. Native double-click detection uses a 400 ms interval
and Manhattan distance of five logical pixels in capture (five physical pixels
in the viewer). Winit wheel lines map to 120 units and pixel deltas use their Y
value. These fallbacks do not establish Qt platform double-click/angle-delta
parity; the original mouse leaf remains Partial with zero completion credit.

Saved shortcut dispatch retains the owning window’s lifetime. Accepted client
exit retires the main route after confirmation; cancelling exit keeps it live.
A closed viewer’s canvas retires keyboard, mouse and partial wheel dispatch,
even if a retained handle is shown again. Fresh bindings and other live viewers
keep their own routes.

Popup jobs can expose a producer-labelled clipboard button, a repeatable callable
button, and a yes/no question. Clipboard clicks read the current full payload,
even while the displayed label awaits refresh. Questions hide while paused;
answering finishes and dismisses the message and returns the Boolean to its live
producer. Callable buttons remain usable after work finishes or is cancelled,
until the message or producer is retired. Action replacements publish immediately.
Native controls use job, producer, question and GUI incarnation tokens, so old
rows, closed owners and prior bindings cannot affect successors. Accepted close
retires pending GUI calls; an already committed answer survives GUI retirement.
The actual Qt action recording is `oracle/fixtures/popup_actions.json`.

Popup geometry reports include the job and GUI incarnation alongside their
measured width. Newly laid-out cards report their initial frame, and moved rows
report again; retired or displaced cards cannot overwrite successor measurements.

Main-window menu titles refresh after page changes while the window is live.
Releasing the window and its binding also releases its pages and thumbnail
loaders, including windows opened by native headless tests.

Speed and Memory now stages the thumbnail-cache byte amount and B/KB/MB/GB/TB
unit, plus its days/hours/minutes timeout. Apply persists the independent values
and immediately enforces them in the main grid; Cancel keeps the saved policy.
The grid keeps a byte-accounted least-recently-used decoded cache across its
pages. Access refreshes the timeout; the existing GUI collection timer also
expires idle entries. Like the reference, admission may exceed the soft limit
by one incoming thumbnail until maintenance trims it. Help > debug > memory
actions > clear thumbnail cache resets the cache and redraws current thumbnails
without asking. Clear, scale round trips and thumbnail-setting changes reject
old decode completions; rebinding or accepted client close permanently retires
the old cache and cancels its staged Options owner and descendants. Raw byte-control
clamping is saved on unchanged Apply; raw subminimum timeout values stay untouched
until their fields are edited. Actual Qt controls, cache boundaries and debug QAction are
recorded in `thumbnail_cache.json` and its PNG.

The main page's sidebar and preview now have drag handles. Each live page keeps
its own sizes across switches and close/reopen; a newly loaded session uses the
global saved defaults without changing session content. Double-click a handle to
hide its pane; hiding clears preview focus while keeping selected files.
Pages > sidebar toggles the current sidebar, saves its sizes immediately, toggles
saving on accepted client exit, or restores saved sizes to every live page.
Cancelling exit saves nothing. A hidden whole sidebar saves width zero and its
last inner preview height; a separately collapsed preview keeps the saved height.
Options > gui pages > preview window stages the global hide preference. Apply
preserves concurrent size saves and the exit switch; Cancel discards the draft.
New pages and restore/reveal consume the saved hide flag. Existing accepted preview
media survives refused SetMedia/clear calls while globally hidden. Each live page
retains its own accepted file and interval across page switches, whole-window hide
and live close/reopen; returning to that page restores its image without counting a
second view. Forgetting the page, loading fresh page owners or accepted client close
retires its state, including late decoder replies. Preview rendering snapshots use a
64MiB soft LRU budget with the shown frame allowed above the limit; an evicted frame
re-decodes its accepted file without starting another interval. The existing two
workers and one latest queued request are shared across all pages. Normal page hide
with a visible splitter ends its interval, and normal show starts its restored
successor interval. An already collapsed splitter also refuses clear/update;
disabling the global flag alone does not replay an earlier refused clear. Pending
and failed-raster acceptance still differs from Qt (see DIFFERENCES.md).

Tag Sort now includes independent opening defaults for search-page and media-viewer
Manage Tags dialogs. Both stage tag/subtag/count, text or count order, siblings/tags,
and namespace grouping in the reference's four-row Options order. Apply saves the
changed context; Cancel preserves it, and reopening restores it. Each local tag
service tab captures its opening sort and offers the same choices within the
Manage Tags window. Switching sort types remembers separate text and count
orders. A later default change affects new dialogs; an open dialog retains its
local sort. Search launch uses the selected files; viewer F3 uses its current file.
Logical storage tags retain their labels, counts and segmented namespace/sibling
colours while sorting can use their sibling ideals. The actual Qt recording
`manage_tags_sort.json` covers 72 combinations and both captured-dialog lifetimes;
`manage_tags_sort_options.png` shows the actual controls.

Speed and Memory > download pages update now stages the four experimental gallery
and watcher list-update controls: seconds/milliseconds with a 250 ms minimum and
denominators 1–99, defaulting to one second and 30. Apply accepts displayed imported
bounds and saves each field independently; Cancel retains raw imports. Only the
visible current downloader page polls its list/totals on the 250 ms GUI timer. Each
deadline uses the currently displayed row count and the saved minimum/denominator;
equality waits for the next tick. Explicit importer actions force an update.
If a render caller still reads the page, the timer defers without consuming its
deadline. A deferred explicit refresh belongs only to that page incarnation.
Highlighted file/job controls, tab progress and close checks stay current while
the list waits. Hidden windows pause list reads, while rebinding or accepted close
permanently retires the old scheduler. The actual Qt controls, save/reopen and
strict/forced gallery/watcher deadlines are recorded in `downloader_update_times.json`
and its reference PNG. Native/model/store regressions and three native snapshots
are authored for hosted validation. Fixture JSON parsing preserves the recorded
floating-point deadlines exactly, including strict equality boundaries.

Tag Presentation > other rendering now includes “OR connecting string (on one
line)”. Apply saves its raw text; Cancel discards the draft. Blank, whitespace,
Unicode and previously saved multiline text survive reopening and legacy import.
This matches the reference editor: Qt currently leaves its custom connector
renderer disabled, so saving this field preserves existing OR labels, colours
and copy/export syntax. Namespace formatting and the OR top-row colour remain
independent live settings. Hidden, cancelled, rebound and closed Options owners
cannot save a connector draft.

Delayed deletion consumers keep a weak identity for their originating page.
Replacing a session releases that page even when its viewer stays open. A live
viewer can still act on its files, while its forgotten source and successor page
receive no row removals. Accepted client exit permanently retires its binding;
a reopened client starts a fresh binding.

Media Playback > system now stages “Apply image ICC Profile colour adjustments”.
It defaults on, imports the saved ClientOptions value, and persists on Apply;
Cancel keeps the original value. Turning it off ignores embedded profiles while
retaining PNG gamma/chromaticity normalization. Existing importers and their
maintenance tools read the saved policy for future image conversions. Existing
pixel hashes and stored thumbnails remain unchanged; explicit thumbnail
regeneration uses the current policy and writes profile-free pixels.

Accepted static preview, browser viewer, archive/delete and duplicate-filter
images refresh after a saved policy change. Sharp tiles refresh at the existing
zoom/pan, without changing focus or starting another viewing interval. Preview
and filter decode requests capture the policy, and old replies cannot populate
new-policy caches or successor owners. Hidden viewers refresh when shown again;
retired Options and image owners remain retired. Native ugoira and WebP players
read the policy for future frame conversions while preserving an already paused
frame and playback position. Actual Qt checkbox staging, save/reopen, decoded
pixels, animation frames, cache notifications and profile-free thumbnail encoding
are recorded in `image_decoder_policies.json` and its reference PNG. Truncated
image loading has no native control or completion claim.


File > open > quick export directory now opens the saved Default export directory
through the existing OS opener. Each click reads the current saved preference;
Options Cancel and one-off manual export destinations leave it unchanged. Portable
relative paths resolve against the database directory, while literal whitespace
and empty imported strings retain the reference's path behavior. A configured
missing path is passed to the opener without creating it. An unset preference
creates the resolved home's hydrus_export directory; an undetermined home reports
the reference message, and a conflicting file reports the creation error without
launching. The action has no picker or confirmation. Hidden, rebound and accepted-
closed main bindings cannot launch or create a directory. Actual Qt QAction,
portable/path/error, Options Cancel/Apply/reopen behavior and menu appearance are
recorded in quick_export_directory.json and its PNG.

### Legacy coloursets

Options > colours stages the independent override checkbox, current default/darkmode choice, and both sets of thirteen RGB roles. The editing tab starts on default even when darkmode is active. Override disables the swatches, while the current-set choice stays available. RGB pickers own OK/Cancel and the colour buttons retain copy/import-hex actions; Apply writes the changed override, current set and individual RGB roles against the latest Store values. Cancel and retired/hidden child callbacks cannot alter successors. Existing imported stores without the typed setting read their retained ClientOptions colours; native settings take precedence.

Saved roles reach the existing thumbnail grid, autocomplete editor/results, active/selected tag lists, manual export tags, preview, viewer and archive/delete/duplicate canvases through owner-local weak observers (at most 250ms). Thumbnail local/remote fill and border use current physical local-file-storage membership, including trash; retained disk bytes are insufficient. Help > darkmode toggles the legacy colourset, showing the reference warning first when override is off. Accepted close, rebind and the final Bound drop retire that action and its notice. Application Palette, stylesheet and OS colour scheme remain independently owned by their style controls.

The real Qt fixture `gui_coloursets.json` records all 26 RGB values, staged/Cancel/reopen/serialization, actual QColorDialog acceptance/rejection, clipboard parsing, four Help cases and thirteen real backend colour consumers. Native/model regressions additionally cover field merges, retained legacy fallback, hidden/rebound owners, physical deletion/restoration and role painting. Source-only checks are complete; hosted Rust/renderer checks and exact-source PNG review remain required.

Media Playback > zoom and position now stages the separate “Preview Viewer default zoom:” choice: default for filetype, 100%, canvas fit, fill horizontally, fill vertically and canvas fill. Apply saves only the changed preview policy; Cancel and retired callbacks preserve it. New imports and retained legacy stores keep the preview override, with native settings taking precedence. Accepted still/poster previews use their real pane geometry and device pixel ratio, the file type’s preview scale rules and centered/clipped bounds. Saving alone leaves the current canvas alone; the next accepted media or actual resize reads the saved default. Cache/ICC replacements keep accepted geometry and viewing intervals. Full viewer zoom policy remains independent. The actual Qt controls and 48 MediaContainer geometries are recorded in `preview_default_zoom.json`; authored model/native persistence, admission and rendered geometry regressions await hosted CI.

Thumbnail appearance now stages the reference fade, blurhash-recovery, renderer
choice and background-file controls. File browsing uses the currently typed
draft, preserves it on picker Cancel, and changes storage only on Apply. Blank
clears the background; whitespace remains a literal path. The image keeps its
own dimensions at the clipped viewport's bottom-right, across scrolling and
resizing. Both old and default-new reference renderers have that background
consumer. Hidden/cancelled/retired Options owners cannot stage the new controls.

Missing physical thumbnails first use the existing read/regeneration path. If
that fails, enabled recovery decodes the file's saved blurhash at the reference
thumbnail resolution (32x32 followed by OpenCV resizing for larger results).
Disabled, absent or invalid blurhashes show the bundled hydrus thumbnail. This
recovery never writes blurry pixels to physical thumbnail storage. A policy
change invalidates cached/in-flight recovery; stored thumbnails remain independent
of the ICC option. Earlier native imports fall back to retained ClientOptions
until a native appearance value is saved.

Pages admit their renderer choice at creation, so Apply affects future pages.
Native decorated-cell snapshots retain image, selection, icons, ratings, banners
and colours while their owned transition runs. The default transition uses the
recorded 13/60-second policy; old-mode frame accumulation uses the older policy.
Fade and renderer-tech parity remain Partial: software rendering applies opacity
to individual primitives, rather than Qt's precomposed bitmap. This work does
not claim whole-bitmap blend fidelity or Qt's separate rendering/layout engines.

Actual Qt evidence is recorded in `thumbnail_appearance.json` and its three PNGs;
the reference painter's inherited new-background method is driven at two scroll
origins. Authored native regressions cover real loader recovery, held policy
results, current typed browse seeds, hidden/cancelled owners, clipped/nonuniform
background pixels, real selection transitions, cached revisits and retirement.
Rust/Clippy/native execution and native PNG inspection are hosted-CI pending.

Accepted client exit also releases the main window's background bitmap; retaining
and showing the closed component cannot restore it. Cancelling exit keeps the
current bitmap, while a fresh binding can load the still-saved path.

The combined colour/thumbnail integration copies all eight saved local/remote and selected/unselected fill/border roles into each owned paint snapshot. Current physical storage membership selects the palette, including collection membership. A colour change clears both old and current copied cells before repaint; the viewport background keeps its saved grid colour behind the clipped image. This integration remains source-only until exact hosted native execution and PNG inspection.

Archive/delete filtering now offers the actual finish deletion scopes: the page's
local context, each current local domain, and an all-local choice when files span
domains. The Files and Trash controls can reduce this to all possible domains or
disable the reference's 1.2-second activation delay for multiple commit buttons.
Single-choice and keep-only finishes stay enabled. Each choice retains its service
keys until acceptance; deletion rechecks identities and current membership inside
the transaction before archiving kept files. Forget asks the reference yes/no
question, and back returns to filtering. Settings stage/Cancel/reopen independently,
merge only changed fields, import real ClientOptions keys and preserve older
retained values when the native key is absent. Closed, rebound, hidden and pending
child owners cannot commit; owned timers stop on resume/close and cannot enable a
successor's buttons. Actual Qt recorded 48 finish cases and both independent
controls; Rust/native regressions and finish PNGs await hosted validation.

Repeated main F12 brings the same active filter and its finish/Forget question forward; it never replaces an unanswered owner. Retired source pages are discarded before opening a fresh filter. Mixed current/deleted search scopes retain their complete context identity and exact label, so their current-only deletion alternatives and activation delay are not collapsed.

## Debug long-text popup

Help > debug > gui actions > make a long text popup publishes two ordinary,
finished popup jobs to the real Store/toaster. The first body gains a random word
from the reference's five-word vocabulary every 200 ms for 62 updates; the second
keeps its “test long title” body while its title grows for the next 62 updates.
The two jobs are immediately dismissible. Repeated invocations own independent
sequences. Merely hiding the main window preserves already published backend
updates, while refusing new launches. Dismissal releases pending updates at the
next live boundary; accepted exit, binding retirement and owner destruction
release the producer's timer and queue. Exit Cancel preserves the live sequence.
The actual Qt recorder captures every setter deadline and presents real popup
widgets; authored native regressions replay the genuine menu, Store and toaster
with an owner-local deterministic clock. Hosted Rust execution remains pending.

Options > tag sort now includes the staged namespace grouping queue. Add and
Edit open owned Enter Text children with the reference's `namespace` default,
allowing blank, `:` and arbitrary raw namespace strings without normalization or
deduplication. Blank and colon rows render as “unnamespaced” and “namespaced”.
Edit captures the first selected row's stable identity, including duplicate rows;
Up/Down and confirmed removal operate on the same queue. Parent Apply is blocked
while a child owns input, Cancel discards the draft, and rebind or accepted client
exit closes the exact children permanently. Hidden or retired callbacks cannot
stage changes. Saved namespace order reaches the existing sidebar/viewer tag-sort
consumers; presentation saves preserve concurrent unrelated fields. Actual Qt
queue/prompt/sort recording and authored model/native regressions cover this
finite scope, including real Enter Text blank/whitespace Apply and Cancel
handlers with their “apply”/“cancel” buttons. Hosted Rust execution and native PNG review remain pending.
