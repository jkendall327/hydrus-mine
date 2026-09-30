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
that had expired. Configuration hydrus-rs doesn't
use yet (downloaders, pages, the GUI's options) is kept verbatim inside the
new store, so later versions can pick it up without a re-import.

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
