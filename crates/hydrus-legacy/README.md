# hydrus-legacy

Read-only, typed access to a reference (hydrus **v688**) client database
directory. It is the source side of the one-time importer (ADR-1): it never
writes, knows nothing about the native store, and turns the reference's
tables and serialised objects into typed Rust values.

```rust
let db = LegacyDb::open("/path/to/hydrus/db")?;   // v688 only
let _snapshot = db.snapshot()?;                    // one consistent view
for service in db.services()? { /* id, key, type, name, typed config */ }
for row in db.mappings(my_tags, ContentStatus::Current)? { let m = row?; /* tag_id, hash_id */ }
let layout = FileLayout::new(&db.file_storage()?, db.db_dir())?;
let path = layout.file_path(&sha256, mime);
```

## Opening

* The four files (`client.db`, `client.caches.db`, `client.mappings.db`,
  `client.master.db`) are opened with SQLite's `mode=ro` and attached as
  `main`, `external_caches`, `external_mappings`, `external_master`, like the
  reference does; the connection also sets `PRAGMA query_only`. A test
  checks every kind of write is refused and the files' bytes never change.
* Only version 688 opens. Older databases get "update it first by running
  hydrus client v688 on it once"; newer ones are refused too.
* **Running alongside the reference client works.** The reference uses WAL,
  where readers and the writer do not block each other; a test holds a write
  transaction open on the fixture while reading it. `LegacyDb::snapshot()`
  keeps one read transaction so a whole import sees one committed state (WAL
  commits spanning several of the four files are atomic per file only).
* **What WAL requires of the directory:** a WAL reader needs `-wal` and
  `-shm` files beside each database. If the client is not running they do
  not exist, and SQLite creates them (the `-wal` empty) and leaves them
  behind; nothing else in the directory changes (tested). The reference uses
  them normally next time, as long as it runs as the same OS user as the
  importer. If the directory is not writable and no `-wal` file exists (so
  no writer can be running), the open falls back to SQLite's `immutable=1`,
  which creates nothing (tested when run unprivileged, as in CI).
  `OpenMode::Immutable` asks for that up front, for backups and read-only
  media, where the files must not change while open.
* If the reference was started with a rollback journal
  (`--db_journal_mode`), readers and the writer block each other; close the
  client first.

## What is read

Big tables stream through `Rows` (an `Iterator<Item = Result<T>>`) using
keyset pagination on the primary key, so memory is bounded however large
the table (mappings at public tag repository scale). Rows arrive in
primary-key order. Small configuration tables are returned whole.

| area | readers |
|---|---|
| services | `services()` (id, key, type, name, decoded settings, raw dictionary), `service_infos()` |
| master | `hashes`, `local_hashes` (md5/sha1/sha512), `namespaces`, `subtags`, `tag_definitions`, `tags` (joined text), `url_domains`, `urls`, `texts`, `labels`, `notes`, `blurhashes`, `perceptual_hashes`, `perceptual_hash_map` |
| files | `files_info`, `forced_filetypes`, `files_with_property` (`has_exif`, `has_transparency`, ...), `current_files`/`deleted_files`/`pending_files`/`petitioned_files` per file service, `deletion_reasons`, `inbox`, `archive_timestamps`, `file_modified_timestamps`, `domain_modified_timestamps`, `viewing_stats`, `deferred_physical_file_deletes`/`_thumbnail_deletes` |
| metadata | `url_map`, `file_notes`, `ratings` (like/dislike and numerical, as stored fractions), `incdec_ratings`, `recent_tags` |
| tags | `mappings`, `tag_siblings`, `tag_parents` per tag service and `ContentStatus` (petitions and pending pairs carry their reason), `sibling_application`, `parent_application` |
| duplicates | `duplicate_kings`, `duplicate_members`, `alternates_groups`, `alternates_members`, `confirmed_alternate_pairs`, `false_positive_pairs`, `potential_duplicate_pairs`, `pixel_hash_map`, `similar_files_search_progress`, `auto_resolution_rule_ids`, `auto_resolution_declined_pairs`, `auto_resolution_actioned_pairs` |
| storage | `file_storage()` (granularity, locations, prefix subfolders, ideal locations, thumbnail override); `paths::FileLayout` computes file and thumbnail paths exactly as the reference does, resolving locations relative to the database directory |
| repositories | `repository_updates`, `repository_unregistered_updates`, `repository_updates_processed`, `repository_hash_id_map`, `repository_tag_id_map`, `remote_thumbnails`; IPFS `service_filenames`, `service_directories`, `service_directory_files` |
| objects | `json_dumps`, `json_dump(type)`, `json_dumps_named` and `json_dumps_hashed` (streamed), `json_dict`, `legacy_options` / `legacy_options_yaml`, `file_maintenance_jobs` |
| typed objects | `services()` configs, `client_api_manager()`, `tag_display_manager()`, `favourite_search_manager()`, `client_options()`, `legacy_options()` |

### Repository processing state

A hydrus repository publishes numbered *update files*; the client downloads
them into the "repository updates" file domain, then processes them.
`repository_updates_N` maps update index to update files,
`repository_unregistered_updates_N` lists update files whose type is not yet
known, `repository_updates_processed_N` records per update file and content
type whether it has been processed, and `repository_hash_id_map_N` /
`repository_tag_id_map_N` (in the master database) map the repository's own
ids to local ids. Carrying these over lets the new client continue syncing
instead of reprocessing (days, for the public tag repository). The fixture
has no repository, so these readers are only checked for their service
guards; the repository service settings are checked against the reference's
defaults.

## Serialised objects

The reference stores configuration as versioned JSON tuples
(`[type, version, info]` or `[type, name, version, info]`).

* `serialisable::SerialisableObject` is a lossless generic tree: `info` is
  kept as `pyjson::PyJson`, except the three generic containers (dictionary
  21, list 26, bytes dictionary 33), whose meta-encoded entries
  (`[0, json]`, `[1, "hex"]`, `[2, nested tuple]`) are decoded so nested
  objects are reachable. Anything not in exactly the reference's shape stays
  verbatim.
* `pyjson` reproduces Python's `json.dumps` byte-for-byte (`", "`/`": "`
  separators, `ensure_ascii`, float `repr`, `NaN`/`Infinity`, big ints), so
  every stored object re-serialises to its stored bytes. The tests check this
  for every object in the fixture, and check float and string formatting
  against samples written by Python.
* `objects` holds typed decoders: service settings (ratings' shape, colours,
  stars and allow-zero; the Client API's port, CORS and so on; repository
  credentials, metadata and pause flags; IPFS), `ClientApiManager` and
  `ApiPermissions` (access keys, names, permits-everything, basic
  permissions, search tag filter), `TagFilter` (with the reference's
  matching rules), `TagDisplayManager` and `TagAutocompleteOptions`,
  `FavouriteSearchManager` (predicates stay generic), `MediaSort`,
  `MediaCollect`, `TagSort`, `LocationContext`, `TagContext`, and
  `ClientOptions` (every typed group — booleans, integers, strings, keys,
  lists, colours — plus everything `/manage_database/get_client_options`
  reports, and favourite tags and tag filters). Favourite tags are the
  `favourite_tags` string list; per-service favourites are
  `suggested_tags_favourites`.

**Versions.** Objects are only re-saved by the reference when they change,
so an object may be stored at an older version than v688 writes. The typed
decoders accept every older version and apply the reference's upgrades,
which here are all small field additions with defaults. Each upgrade is
tested against what the reference's own upgrade code produces
(`upgrade_vectors` in the expectations). The generic model keeps objects at
their stored version; `SerialisableObject::upgraded` applies the container
upgrades (dictionary v1, list v1/v2). The client options are the exception:
the v686→v687 database update re-saved everyone's options, so a v688
database always has version 8 and nothing else is accepted.

## What is skipped, and why

Derived data is never imported (ADR-1); the native store rebuilds its own
(ADR-6). Not read:

* the whole caches database except `file_maintenance_jobs`: autocomplete
  counts, display-tag caches, subtag and note full-text indexes, local
  hash/tag lookup caches, sibling/parent lookup caches;
* `service_info` (cached counts), the similar-files search tree
  (`shape_vptree`, `shape_maintenance_branch_regen`,
  `shape_search_cache_numbers`), the duplicates auto-resolution count cache
  and search queues;
* maintenance bookkeeping: `analyze_timestamps`, `vacuum_timestamps`,
  `last_shutdown_work_time`, `deferred_delete_tables`.

`file_maintenance_jobs` is a work queue rather than user data. It has a
reader so the importer can re-queue pending work (regenerating thumbnails,
integrity checks the user asked for); skipping it loses only work not yet
done. `shape_search_cache` (how far the similar-files search has got per
file) is derived but expensive to recompute, so it has a reader too.

Every table in the fixture is classified as primary or derived by the
oracle script, and a test fails if a table appears that is not classified,
or a primary table has no reader.

## Reference behaviour worth knowing

* The `json_dumps*` tables keep an object's type and version in columns and
  only `info` in `dump`; `services.dictionary_string` holds a whole tuple;
  and the client options' `info` is itself a whole dictionary tuple.
* The reference's own re-serialisation of the stored client options is not
  byte-identical to what is stored (it merges in defaults), which is why the
  raw dump is kept alongside any decoding.
* Python tuples become lists on a load/save cycle, so the same setting may
  be stored as plain JSON or as a `SerialisableList` depending on history;
  decoders accept both. The `floats` options group holds integers too.
* Set-valued fields (location contexts' service keys) are stored in
  arbitrary, hash-seed-dependent order.
* `hashes` holds every sha256 the client knows, not just files: pixel hashes
  and repository update files too. `blurhashes` has rows with no blurhash
  (audio, archives). Archives and other types without thumbnails have no
  thumbnail file.
* The IPFS tables' `directory_id` is a *text* id (the directory multihash).
* Numerical ratings are stored as fractions in `0.0..=1.0`; converting to
  stars uses Python's round-half-to-even.
* A repository's legacy `paused` setting overrides the newer
  `update_downloading_paused`/`update_processing_paused` when both exist.
* Client API permissions v1 had no "permits everything" flag; the upgrade
  infers it from holding the ten permissions that existed at the time.
* The legacy `options` table is PyYAML output read with YAML 1.1 rules
  (`yes` is a boolean), with `!!python/tuple` and `!!binary` tags and a
  `None` key in the namespace colours; it is parsed with the reference's
  scalar resolution rather than a YAML 1.2 loader.

## Tests

The tests need no Python. `oracle/dump_legacy_expectations.py` extracts the
`basic` fixture and writes `oracle/fixtures/legacy_db/basic.expected.json`
using the reference's own classes; `tests/` compares everything against it:
full contents of every primary table, services and their defaults, API
permissions and tag filter verdicts, client and legacy options as the API
reports them, stored object identities, byte-for-byte re-serialisation of
every stored object, upgrade vectors, file paths, and Python JSON
formatting. `tests/access.rs` covers the read-only and concurrency
guarantees. `hydrus-testkit::legacy_fixture` extracts fixture databases for
any crate's tests.
