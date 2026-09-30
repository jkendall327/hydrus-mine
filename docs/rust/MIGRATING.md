# Moving a hydrus install to hydrus-rs

hydrus-rs reads a hydrus **v688** install once and builds its own store from
it. Your old install is never modified (unless you ask for `--files move`),
so you can import, try it out, delete the new store and import again as often
as you like.

## Steps

1. Update the old client to v688 and close it cleanly.
2. Build hydrus-rs: `cargo build --release` (the binary is
   `target/release/hydrus`).
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
User-Agent your browser extension set) and every site's cookies, less those
that had expired. Your import options come across too (the defaults for
each kind of import, per URL class, and your favourites), so the Client
API's file imports already follow your file filtering and destination
settings, and your downloaders' parsers and gallery URL generators are
converted for the downloader (a definition that can't be converted is
reported and kept). Subscriptions come across with their settings, their
queries' timing and state, and each query's full history (the files and
gallery pages it has seen, with their status and notes), so nothing is
downloaded twice; `hydrus serve` checks them on their schedule, and `hydrus
subscriptions <store> list` shows them. Duplicates auto-resolution rules come
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
before its first run after a move, and it is linked afresh. The downloader
pages open in the session hydrus opens with ("last session" unless you
changed it) come across as queues named after their pages: a URL
downloader page as one queue, a gallery downloader page as one per search,
a watcher page as one per watcher, each with every URL, file and gallery
page it held and whether it was paused; `hydrus serve` carries on with
them, and `hydrus queues <store> list` shows them. Configuration hydrus-rs
doesn't use yet (the rest of your sessions, the GUI's options) is kept
verbatim inside the new store, so later versions can pick it up without a
re-import.

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

Hardlinks need the new store on the same filesystem as the old files; if
your files are spread across drives, use `copy` or `in-place`.

## Safety

- The import writes to a scratch file and only renames it into place when
  every table's row count has been checked against the source.
- The old database is opened read-only (and immutable when the client isn't
  running), and its files' sizes and modification times are checked
  afterwards to prove nothing changed.
- Files deleted in hydrus-rs are removed from disk by a background job only
  after the deletion has committed, never while a re-import of the same file
  is in progress, and never when the media is shared with the old install.
