# Native store design

`hydrus-store` owns all persistent state except media files on disk. This
document describes the schema and the rules that keep it consistent. It is
the reference for anyone touching SQL.

## One file, two kinds of table

Everything lives in a single SQLite database, `hydrus.db`, in WAL mode.

* **Primary tables** hold facts only the user can create: files, tags,
  mappings, ratings, notes, ... They are imported from reference databases
  and never recomputed.
* **Derived tables** are prefixed `cache_`. They are pure functions of the
  primary tables (and the in-memory tag display graph, which is itself a pure
  function of primary tables). Each has exactly one *rebuild* path and one
  *incremental* path, and property tests assert they agree (ARCHITECTURE.md,
  ADR-6). `hydrus maintenance <store> rebuild-caches` drops and rebuilds them
  all.

One file keeps transactions atomic across everything (the reference splits
four files, which makes cross-file transactions non-atomic in WAL mode).

## Identity

Interned master data (hashes, namespaces, subtags, tags, url domains, urls,
texts, note labels, note bodies, perceptual hashes) use integer primary keys.
The importer preserves the reference's ids, so ids in an imported database
match the reference database they came from; the Client API exposes file ids
(`file_id` = `hash_id`), so clients' cached ids stay valid across migration.

Service ids are `AUTOINCREMENT` so a deleted service's id is never reused
(per-service tables are named by id).

## Services

`services(service_id, service_key, service_type, name, config)`. `config` is
JSON for the typed per-kind settings (`ServiceKind` in `services.rs`). The
built-in services and their keys are identical to the reference (see
`hydrus_core::service::builtin_keys`).

## Files

* `files`: one row per file we have *metadata* for (local, deleted, or known
  from a repository): size, mime, dimensions, duration, frames, audio, words,
  forced mime, file-modified time, pixel hash, blurhash, and a bitset of
  content flags (exif, icc profile, human-readable metadata, transparency,
  xmp, iptc, software source) with partial indexes for the rare flags.
* `file_domain_{current,deleted,pending,petitioned}(service_id, hash_id, ...)`:
  membership of files in file domains, including the reference's umbrella
  domains ("combined local file domains", "hydrus local file storage",
  "deleted from anywhere"), which are materialised exactly as the reference
  materialises them. "All known files" is not materialised.
* `file_inbox`, `file_archived(hash_id, archived_ms)`,
  `file_deletion_reasons`, `file_domain_modified(hash_id, domain_id, ms)`,
  `file_viewing_stats`, `file_urls`, `file_notes`, `ratings`,
  `ratings_incdec`, perceptual hashes.

## Tags

* `mappings_<sid>_{current,deleted,pending,petitioned}(tag_id, hash_id)` per
  tag service, `WITHOUT ROWID`, primary key `(tag_id, hash_id)` plus a
  `(hash_id, tag_id)` index. Per-service tables keep public-tag-repository
  scale indexes separate and make dropping a service O(1).
* `tag_siblings` and `tag_parents` hold every service's relations with a
  status column; `tag_display_application` says which services' relations
  apply to which tag service's display, in priority order.

### Display semantics

For a tag service S, the *display graph* is built from the relations of S's
applicable services, in application order, current (minus petitioned) then
pending:

1. Siblings form chains toward an *ideal*. A pair whose bad tag already has a
   good tag is ignored (first wins); pairs that would close a cycle are ignored.
2. Parent pairs are first rewritten through S's sibling ideals, then closed
   transitively into descendant → ancestors; pairs that would create a cycle
   are ignored.

A storage tag T displays as `ideal(T)` plus `ancestors(ideal(T))`. Searching
for tag X in S finds files with any storage tag T whose display set contains
`ideal(X)`. "All known tags" means: do that per real tag service and union.

Graphs are small (even for the public tag repository) and are held in memory,
rebuilt from the relation tables whenever relations or application change.
Display tags for a file are computed on the fly from its storage tags; there
are no materialised display mapping tables.

## Derived data

* `cache_tag_counts_<sid>(domain_id, tag_id, current, pending)`: number of
  files in file domain `domain_id` (or "all known files", id of that service)
  having storage tag `tag_id` in tag service `sid`.
* `cache_display_counts_<sid>(...)`: the same for display tags. A file counts
  once per display tag however many of its storage tags map to it.
* `cache_subtag_words(word, subtag_id)`: the words of every subtag's
  *searchable* form (punctuation like `_-()[]` becomes a space), for
  autocomplete word-prefix search via B-tree range scans. Tokenising happens in
  Rust so it reproduces the reference's FTS4 "simple" tokenizer exactly
  (ASCII alphanumerics and every codepoint ≥ 128 are word characters).
* `cache_integer_subtags(subtag_id, value)`: subtags that are integers, for
  `system:tag as number`.
* `cache_note_fts`: FTS5 index of note text.

## Connections and transactions

A single writer thread owns the read-write connection. Writers submit
closures; the writer runs each inside a `SAVEPOINT` within a batch
transaction, commits the batch, then acknowledges each closure's result
(group commit). A failed closure rolls back only its own savepoint.

Readers borrow connections from a pool and see a consistent WAL snapshot.

### Cached domain files

Most searches are limited to a file domain and sorted by import time, and
reading either costs a row per file in the domain. `domains.rs` keeps each
domain's files (as a bitmap) and its import order in memory, keyed by
`domain_generation`, a one-row counter that every write changing domain
membership or import times increments in the same transaction
(`ContentWriter` does this; a write to the domain tables that goes around it
must call `domains::changed`). A read looks up the counter inside its own
transaction, so it only uses cached data that matches its WAL snapshot, even
while writes commit around it.

A search loads a domain into the cache once it would otherwise check more
than a sixteenth of the domain's files one by one (a check costs about eight
scanned rows, and a loaded domain serves later searches too). To decide that
without counting the domain every time, the cache remembers each domain's
size as last seen, whatever the generation.

### Cached potential pairs

Every duplicate filter request (counting pairs, fetching a batch, picking a
random group) looks at all potential pairs, and reading them joins four
tables per pair. `duplicates/cache.rs` keeps them, with what ordering needs
about each pair's kings (size, resolution, pixel duplicates), under
`duplicates_generation`, which every write to pairs, groups or a grouped
file's properties increments in its transaction. Only reads fill either
cache: a write's uncommitted state could be rolled back and its generation
number reused.
