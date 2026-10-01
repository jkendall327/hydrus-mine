# Moving a hydrus install to hydrus-rs

hydrus-rs reads a hydrus **v688** install once and builds its own store from
it. Your old install is never modified (unless you ask for `--files move`),
so you can import, try it out, delete the new store and import again as often
as you like.

## Steps

1. Update the old client to v688 and close it cleanly.
2. Build hydrus-rs. Install Rust with [rustup](https://rustup.rs) (the
   repository pins the version, which rustup fetches on the first build),
   and a C toolchain: on Windows, Visual Studio's Build Tools with "Desktop
   development with C++" (rustup's installer offers them); on macOS, `xcode-select
   --install`; on Linux, a C compiler, `pkg-config` and fontconfig's
   development files for the desktop client (e.g. `build-essential
   pkg-config libfontconfig1-dev`). SQLite is built in. Then, in the
   repository, `cargo build --release`: the binaries are
   `target/release/hydrus` and the desktop client `target/release/hydrus-gui`
   (`.exe` on Windows). `cargo build --release -p hydrus-cli` builds just the
   command line. Video and audio need `ffmpeg` on your `PATH` (for their
   metadata and thumbnails, as hydrus does); images don't. To play video in
   the desktop client, it needs libmpv, as hydrus does: on Linux your
   distribution's `libmpv2` (or `mpv`) package; on Windows `libmpv-2.dll`
   or hydrus's `mpv-2.dll` next to `hydrus-gui.exe`; on macOS `brew install
   mpv`. Without it, video shows its thumbnail. Your `mpv.conf` comes
   across with the import.
3. Import:

   ```sh
   hydrus import-legacy /path/to/hydrus/db /path/to/new/store
   ```

   `/path/to/hydrus/db` is the directory holding `client.db`. The new store
   directory must not exist yet (or be empty).
4. Serve the Client API with the same port, access keys and services as
   before:

   ```sh
   hydrus serve /path/to/new/store
   ```

## What carries over

Everything in the database: files and their domains, tags on every tag
service, siblings and parents and how they apply, notes, ratings, URLs,
timestamps, viewing statistics, duplicate and alternate groups, Client API
access keys and their permissions, and services with their keys (so tools
that remember service keys keep working). Of the options, those hydrus-rs
already uses come across as its own settings: thumbnail size, favourite tags,
viewing statistics, URL classes, autocomplete rules, and the duplicate
filter's batch size and metadata merge options (what "this is better" copies
or moves between files), and the network side: custom HTTP headers (e.g. a
User-Agent your browser extension set), every site's cookies, less those
that had expired, your bandwidth rules with their recent usage (so
today's limits carry on), and your connection options (timeouts, retries,
how many requests at once, HTTPS verification, proxies, and how long the
downloaders and subscriptions wait after an error). The trash keeps its limits (how old and how big
it may get before files in it are deleted for good), and `hydrus serve`
applies them hourly, as hydrus does; files deleted for good go to the
recycle bin if you had hydrus send them there. If you had hydrus lock
archived files against deletion, the lock comes across too: archived files
can still go to the trash, but aren't deleted for good (by the Client API,
by emptying the trash, or by a duplicate decision) until they are back in
the inbox, and your options to inbox the files duplicate decisions delete
apply. How tags are shown comes across for the GUI: your tag display
filters, whether namespaces are shown, the namespace connector,
underscores and emojis, your namespace order, your tag list sorts and
your namespace colours; how the media viewer zooms (your zoom levels, each
file type's scale up and scale down rules, where zooming centres and the
default zoom); and
your lock password, which `hydrus-gui` asks for before it opens, and your
favourite searches (a favourite searching for something hydrus-rs can't is
reported and left out). How files are handled
comes across as well: where in a video its thumbnail is taken from, whether
zips are checked for comic book archives, what counts as transparency, and
whether hydrus leaves files' permissions alone (for drives that refuse
them). Your import options come across too (the defaults for
each kind of import, per URL class, and your favourites), so the Client
API's file imports already follow your file filtering and destination
settings, and your downloaders' parsers and gallery URL generators are
converted for the downloader (a definition that can't be converted is
reported and kept). Subscriptions come across with their settings, their
queries' timing and state, and each query's full history (the files and
gallery pages it has seen, with their status and notes), so nothing is
downloaded twice; `hydrus serve` checks them on their schedule, and `hydrus
subscriptions <store> list` shows them. What you had paused from hydrus's
"network > pause" menu (subscriptions, all new network traffic, the
downloader queues) stays paused: `hydrus pause <store>` shows what is, and
`hydrus resume <store> subscriptions` (or `network`, `queues`...) resumes
it. The file maintenance hydrus had queued comes across (its v682 update
queued metadata checks for nearly every image, for example): `hydrus
maintenance <store> jobs` lists it, `hydrus serve` works through it at
hydrus's pace (your throttle options come across), and `hydrus maintenance
<store> files` runs it all now. Every kind of job runs, as hydrus's
would: reading files' metadata again (a file whose type has changed is
renamed), checking their metadata flags, regenerating their hashes,
thumbnails and blurhashes, and the integrity checks. A missing or damaged
file is listed in the store's `missing_and_invalid_files` folder with its
tags and URLs, and, as the job says, moved there, downloaded again from its
URLs (in a URL queue named "missing files redownloader", which `hydrus
serve` runs), or its record removed (an archived file the delete lock holds
goes to the trash instead). With `--files in-place`, files are copied,
never moved or deleted, so the old install keeps them.
Duplicates auto-resolution rules come
across with their progress: every pair's status for every rule (searched,
tested, waiting for your approval, denied) and each rule's log of what it
did, so no rule redoes its work; `hydrus duplicates <store> rules` shows
them, and `hydrus serve` carries on running them, including rules that
compare the files' content (visual similarity, jpeg quality). Import folders
come across with their sidecar routing, filename tagging, actions and
schedule, and the files each has already seen, so nothing is imported twice;
`hydrus serve` checks them when they are due, and `hydrus import-folders
<store> list` shows them. A folder's path (and where it moves files) is kept
as it was, so it must be where the new install can reach it. Export folders
come across with their search, naming phrase, sidecars and schedule (and the
export naming options); `hydrus serve` runs them when due, and `hydrus
export-folders <store> list` shows them. A symlinking export folder's existing
links still point into the old install's file storage: with `--files
in-place`, `hardlink` or `copy` they keep working (as long as the old files
stay), but after `--files move` they are broken, and, as in hydrus, a
broken link where an export goes makes the folder fail. Empty such a folder
before its first run after a move, and it is linked afresh. The session
hydrus opens with ("last session" unless you changed it) comes across as
the session `hydrus-gui` opens with: its pages and notebooks in order, each
search page with its search, sort and the files it showed. Its downloader
pages' work comes across as queues named after their pages: a URL
downloader page as one queue, a gallery downloader page as one per search,
a watcher page as one per watcher, each with every URL, file and gallery
page it held and whether it was paused; `hydrus serve` carries on with
them, and `hydrus queues <store> list` shows them. A duplicates page
comes across with its search, pair sort and group mode, and launches the
duplicate filter (a store imported before duplicates pages were read
opens its duplicates pages too, from the copy kept of them). Pages of kinds hydrus-rs doesn't open yet (import from
disk, simple downloader...) are kept as they were stored. Your other saved
sessions come across under their names (hydrus's own "last session", if
you opened with another, as "last session (from hydrus)"), for the page
chooser's "sessions" button: their search and duplicates pages as above,
but their downloader pages only kept, making no queues, since hydrus only
runs a session's downloads while it is open. Configuration hydrus-rs
doesn't use yet (the GUI's options) is kept verbatim inside the new store,
so later versions can pick it up without a re-import.

Client API access keys come across, so your browser extension and other
tools keep working. `hydrus api-keys <store>` lists, adds and removes keys;
to connect a new tool the way hydrus's "review services" dialog does, run
`hydrus api-keys <store> listen` while `hydrus serve` is running, then use
the tool's "request new API key" button and accept the request.

Custom assets in the install's `db/static` folder (your own rating star
shapes, for instance) are copied into the new store's `static` folder.

Autocomplete counts and other caches are rebuilt rather than copied.

## Media files: `--files`

| mode | what happens | space | old install afterwards |
|---|---|---|---|
| `hardlink` (default) | new directory entries for the same files | none | unaffected; both installs independent |
| `copy` | a full copy | as much again | unaffected |
| `move` | the files move to the new store | none | left without its files |
| `in-place` | the new store uses the old install's files | none | shares its files; hydrus-rs never deletes any |

Each drive's files stay on it. Storage locations on the new store's drive
come into its `client_files`; those on another drive go into a new
`<location>-hydrus-rs` directory beside the first of them there (for
`D:\hydrus\client_files`, `D:\hydrus\client_files-hydrus-rs`), which becomes
one of the store's storage locations with the weight and size limit the
old ones had. So hardlinks work wherever your files are, and `copy` and
`move` never fill one drive with another's files.

Only files the database says are stored come across, with their
thumbnails. Files hydrus had deleted but not yet cleared from disk, and
anything else in its media folders, are left where they are (`move` leaves
them in the old folders).

## Safety

- The import writes to a scratch file and only renames it into place when
  every table's row count has been checked against the source.
- The old database is opened read-only (and immutable when the client isn't
  running), and its files' sizes and modification times are checked
  afterwards to prove nothing changed.
- Files deleted in hydrus-rs are removed from disk by a background job only
  after the deletion has committed, never while a re-import of the same file
  is in progress, and never when the media is shared with the old install.
- `hydrus serve` won't start if a media location looks missing (as when a
  drive isn't mounted), rather than write new files where they'd be hidden.
  Only one `hydrus serve` runs on a store at a time, and the commands that
  do its work themselves (`purge`, running an import or export folder or
  the duplicates rules) refuse to while it runs. It stops cleanly on Ctrl-C
  or SIGTERM, so it can run as a service. As in hydrus, an import refuses
  a file when its disk has under 100 MB free (or the copy fails), and
  pauses the subscriptions, file queues and import folders; `hydrus resume
  <store> subscriptions` (and `file-queues`, `import-folders`) starts them
  again.
