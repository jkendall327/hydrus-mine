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

Above the tabs is the reference's menu bar (`src/main_menu.rs`, checked
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
  latest first, to show one again; refresh; appending a saved session, and
  deleting one (asking first); the page chooser; a new search page on each
  local file domain, the trash or a file repository; new URL, watcher and
  gallery pages, a page of pages and a duplicates page; and clearing every
  watcher page's highlight;
- database: whether file maintenance works in idle and normal time;
- network: every pause switch hydrus has (all new network traffic,
  subscriptions, all paged importer work, file importing, gallery
  searching, watcher checking), which the daemon obeys;
- pending: each repository's content to upload, and forgetting it (asking
  first);
- help: the help, links and changelog in the browser, and advanced mode
  (which adds hydrus's advanced entries).

File > options opens the options window (`src/options.rs`), as the
reference's "manage options" dialog: its pages listed on the left as
hydrus lists them (by name, "advanced" last), the page chosen on the
right, each option its label and then its control, in the page's titled
boxes, as the reference's dialog lays them out (checked against the
running reference's dialog, recorded by `oracle/record_options_dialog.py`).
It has the options hydrus-rs honours, so far on twenty pages: audio,
connection (retries, timeouts, job limits, the halt on a domain's errors,
HTTPS checks and proxies), downloading (gallery, subscription and watcher
waits, the default file limit, highlighting, the pause and stop
characters, short summaries' counts, the waits after errors, and two
debug switches), duplicates (the duplicate filter's batches and its
comparison score weights, and the duplicates page opened on files),
exporting, file sort/collect (the default and secondary sorts, each a
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
file summary), ratings, tag presentation, tag sort (the search pages' and
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
text that may be none has the reference's "none" box. Above the pages is
the reference's search box ("Search options... (Experimental!)"): as it
is typed in, it suggests the box titles and options whose text has what
was typed in it (ignoring case), as "text (page)", ten at a time; the
arrows, enter or a click choose one, which shows its page with that row
highlighted, as the reference's does (and the page list brought round to
it). The daemon picks up the connection, downloading, maintenance and
thumbnail options within a second of "apply". Changes wait for "apply", which writes them
together (and shows again what they change, such as the tab names);
"cancel" or escape forgets them. A value that can't be had (slideshow
durations that aren't numbers) is left as it was and said in a popup, as
the reference says it.

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
while it is open, they join its list). It parses the paths given, folders
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
place of the reference's "add files"/"add folder" pickers, and "add
tags/urls with the import >>" is greyed out until the import's metadata
options are ported.

A gallery downloader page is the reference's too. Its "gallery
downloader" box says how its searches stand ("2 queries - 4/6", and
"waiting for new queries" with none) and lists them as the reference's
list does: query, source, files and search status (the reference's pause
and stop characters), status ("DONE", "working", "pending"), items ("2 -
1Ign") and added ("5 minutes ago"), sortable by any column. A double-click
on a search, or its highlight button, shows it: the page then holds that
search's files, and the "highlighted query" box under the list has its
"imports" and "search" boxes, each with its live status line, its pause
button and its download in progress, as a URL page's do; the
clear-highlight button empties the page again. The list's buttons pause
or resume the selected search's files or its search, retry its ignored or
failed files, and remove it, asking first as the reference does ("Remove
the 1 selected queries?", saying how many are still working and that
the page will be cleared if it was shown). Queries typed into its box
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
at first, and by any column. Its buttons highlight a watcher (or a
double-click does), clear the highlight, pause or resume its files or its
checking, check it now, retry its ignored or failed files, and remove it,
asking as the reference does ("Remove the 1 selected watchers?", saying
how many still work or aren't yet DEAD). Thread URLs typed into its
"watcher url" box, or pasted, one a line, become watchers with the page's
checker and import options; one the page watches already isn't watched
twice, and the first new one is shown if hydrus's option to highlight new
watchers is on. The "highlighted watcher" box has its thread's subject
and URL, its "imports" (files line and pause, file log status and
progress, download in progress) and its "checker": how fast the thread
was getting files ("at last check, found 5 files in previous 1 day"),
checking's pause, when it checks next ("next check in 4 minutes",
"checking imminently") or what it is doing, a "check now" button, its
check log's status and its check in progress.

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
`_GetFileSystemPredicates` does (the reference also offers the system
predicates that open an editor; not yet). Typing in the search box lists
the matching tags with their counts (display tags, in the page's file domains
and tag service; the exact match first, then the most used), up and down move the
highlight, and enter adds the highlighted tag, or the text as typed for a
system predicate; a leading hyphen excludes. Predicates are listed as the
reference writes them (`system:width>1920` shows as `system:width > 1,920`),
and double-clicking one removes it. Under the search box, the reference's
pause/play button ("searching immediately", or "search paused", when a
changed search waits to be searched; ctrl+i in the search box switches it). Below them, as the reference's
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
the search box lists your favourite searches in hydrus's folders and
order; choosing one loads its domains, tag service, predicates, sort and
collect into the page (the collect at once, on the files shown), searching
if it is synchronised (saving and editing favourites isn't here yet). The lock button beside them
locks the page's search to a `system:hash` of the files in view (asking
first, as hydrus does, unless the search is empty or already that hash);
a page opened on files ("open in a new page") starts locked. A locked
page shows "search locked" and a "Locked at N files." button, which
unlocks it, in place of the search box and predicates; it doesn't search
on refresh, and files removed from it leave the hash, unless its cog
says otherwise. Locks come across from hydrus's sessions and are kept in
ours. The matching files' thumbnails fill the grid, newest import
first. The status bar says what the reference's does (`src/status.rs`,
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
pixels first, so it is drawn pixel for pixel.
Thumbnails are selected as in the reference's grid (v688's default one;
`src/selection.rs`, checked step by step against that grid driven in the
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
trash physically, delete physically and undelete; manage → tags; and
urls → open in browser, open in a new page, or copy the focused file's
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
in the menu) and file ids. Not yet: the embedded metadata window,
clearing deletion records, the other manage entries,
locations, urls → manage and force metadata refetch, open's custom
similarity distance, and
share's exporting and copying of bitmaps. Double-clicking a thumbnail opens the media viewer in its own
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
now and undelete for a file in the trash; manage → tags; urls, open and
share, as the thumbnails' menu has them for the file alone; and the
player ("This is a MPV Embed Player."). Not yet in it: locations, and
manage's ratings, notes, times, force filetype and viewing stats. The
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
(`src/info_lines.rs`, `GetPrettyMediaResultInfoLines`: size, type,
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
(or "apply") writes them, escape forgetting them. As the reference's
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
quality, alternates, not related, skip, go back. As in the reference, a
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
options. The model is `src/duplicate_filter.rs`, tested in
`tests/duplicate_filter.rs` with the window drawn headless.
`crates/hydrus-gui/tests/search_page.rs` drives the page and
`tests/session.rs` a saved session, and both draw the window headless (the
screenshots land in `target/tmp/`). Not yet: the reference's menu of
closed pages (Ctrl+U reopens them one at a time), managing tags on tag
repositories (pending and petitioning), dragging thumbnails, the rest of
the thumbnails' menu, system
predicates in the autocomplete, the rest of the viewer's hover frames
(the top one's zoom options, volume, shortcuts and view options menus,
window move and embedded metadata buttons, and its tooltips; editing,
copying and hiding notes), the volume shortcuts
other than the global mute, the scanbar's buffering
shading, playing animated JPEG XL, downloader
pages' file and search log windows and import options buttons, and a
download's cog and error menus (bandwidth rules, the last error); in
the duplicates page, editing its search and the preparation and
auto-resolution tabs; in the duplicate filter, the custom action,
deleting from the filter, the hover frames, and
reviewing auto-resolution's pending
pairs; the viewer's other zoom shortcuts (fill, max, the zoom menu), its
zoom and pan locks, and "open externally" for the file types shown with
that button (their thumbnail fills the window instead).

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
   software renderer, which runs headless.
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
