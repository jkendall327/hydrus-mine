# hydrus-rs architecture

This is a from-scratch Rust implementation of the hydrus client. The Python
code in `hydrus/` is the **reference implementation**: we use it as a spec and
as a test oracle, but we do not copy its structure.

## Goals, in priority order

1. **Your data carries over, safely.** A one-time, read-only importer brings an
   existing hydrus v688 install into the new format. It never writes to the
   source install, so you can import, trial, throw away, and re-import as often
   as you like.
2. **The Client API stays compatible.** Tools built for the hydrus Client API
   (browser extensions, downloaders, web UIs) must work unchanged. This is the
   hardest compatibility contract and it is tested against the real Python
   client.
3. **Fast.** Search, autocomplete, thumbnail grids and imports should be
   interactive at public-tag-repository scale (≈10⁹ mappings).
4. **Maintainable and yours.** Idiomatic Rust, strong types, explicit
   ownership of every piece of state, and no bug-for-bug compatibility with
   ten years of accumulated workarounds.

## Decision log

### ADR-1: New native storage format + one-time importer (not in-place compatibility)

The reference database is four SQLite files. Roughly half of its ~280 tables
are derived caches (per file-domain × tag-service cross products, maintained
incrementally in Python for performance). Staying byte-compatible would mean
replicating every cache's exact maintenance semantics so the Python client
could keep using a database the Rust client had written.

We don't need that: migration can be one-way. So:

- The native store is a clean SQLite schema owned by `hydrus-store`, with its
  own forward-only migrations.
- `hydrus-legacy` reads (only reads) the reference format, and the importer
  copies **primary** data across. Derived data is never imported; the native
  store rebuilds its own.
- Interned ids (hash ids, tag ids, ...) are preserved during import. That keeps
  the importer a set of fast bulk copies and makes cross-checking trivial.
- Service keys are preserved exactly: API clients store them.

### ADR-2: File storage layout is unchanged

Media files stay at `<location>/f<hh>/<sha256 hex><ext>` and thumbnails at
`<location>/t<hh>/<sha256 hex>.thumbnail` (with the reference implementation's
deeper nesting for higher granularity). Libraries are often terabytes; the
importer can hardlink (default, zero extra space, independent directory
entries), copy, or move them rather than transcoding anything.

### ADR-3: The Client API is tested black-box against the real Python client

`oracle/` holds Python tooling that boots the unmodified reference client
headless (`QT_QPA_PLATFORM=offscreen`), enables its Client API, and replays
scripted scenarios over HTTP, recording every response. The Rust server
replays the same scenarios against an imported copy of the same database and
must produce the same responses, modulo an explicit, reviewed list of
normalisations (timestamps, ordering of unordered collections, ...).

Recordings are checked in, so CI does not need Python. Re-recording is a
deliberate act (`oracle/README.md`).

### ADR-4: Constants and behaviours are fixture-checked, never hand-copied

Any table that must match the reference implementation (file type codes,
service types, tag cleaning, ...) is tested against JSON dumped from the
Python code (`oracle/fixtures/`). Hand-typed constants have already been caught
wrong by these tests.

### ADR-5: Concurrency model

- One **writer** thread owns the read-write SQLite connection. Writes are sent
  to it as typed commands; it batches concurrent commands into one
  transaction (group commit) and acknowledges each after commit, so an API
  call that returns has durably happened.
- A pool of **reader** connections serves queries concurrently thanks to WAL.
  Readers never block on writers or each other (the reference implementation
  serialises every read and write through one connection).
- Tag sibling/parent graphs are small, hot and read-mostly: they live in
  memory behind an `ArcSwap`-style snapshot that the writer republishes after
  changes. Display tags for a file are computed in memory from storage tags.
- CPU-heavy work (hashing, decoding, thumbnailing, perceptual hashes) runs on
  a rayon pool. The HTTP server is tokio/axum; blocking work is dispatched
  off the async runtime.
- Change notifications are a typed broadcast channel, not stringly pubsub.

### ADR-6: Derived data is explicit and provably consistent

Every derived table (autocomplete counts, text indexes, similarity trees, ...)
has exactly two code paths: *rebuild from primary data* and *apply an
incremental change*. Property tests generate random operation sequences and
assert `incremental == rebuild`. A maintenance command can drop and rebuild any
derived table, so a derived-data bug is never data loss.

### ADR-7: Serialised objects are typed

The reference stores configuration as versioned JSON tuples with per-type
upgrade chains (163 types). The native store uses serde types with an explicit
schema version per record. The legacy reader decodes the reference tuples;
unknown or not-yet-supported objects are preserved verbatim in a
`legacy_objects` table so nothing is lost and support can be added later.

## Crates

| crate | role |
|---|---|
| `hydrus-core` | Domain vocabulary: ids, hashes, file types, services, tags, time. No I/O. |
| `hydrus-legacy` | Read-only access to reference (v688) databases and serialised objects. |
| `hydrus-store` | Native SQLite schema, migrations, writer actor + reader pool, derived data. |
| `hydrus-media` | File type detection, metadata, thumbnails, perceptual/pixel hashes, blurhash. |
| `hydrus-search` | Predicates, the system-predicate text parser, query planning and execution. |
| `hydrus-api` | The Client API HTTP server. |
| `hydrus-cli` | The `hydrus` binary: `serve`, `import-legacy`, maintenance commands. |
| `hydrus-testkit` | Dev-only test helpers: extracting fixture databases, loading fixture JSON. |
| `xtask` | Project automation: parity ratchet, conformance runs, benchmarks. |

The GUI is deliberately last; see `DECISIONS_NEEDED.md` for the open choice of GUI technology.

## Testing layers

1. **Unit and property tests** in every crate.
2. **Fixture tests** against reference outputs in `oracle/fixtures/`.
3. **Conformance tests**: Client API scenario recordings (ADR-3).
4. **Migration tests**: import reference-built fixture databases and check
   every imported fact through the public API.
5. **Invariant tests**: incremental derived data equals rebuilt (ADR-6).
6. **Benchmarks**: criterion micro-benchmarks, plus macro benchmarks that
   compare Python and Rust on the same synthetic library.

## Parity ratchet

`parity/manifest.toml` enumerates the user-visible surface (API endpoints,
system predicates, file types, services, maintenance jobs, ...), each with a
status. `cargo xtask ratchet` recomputes the numbers from the manifest and the
conformance results, and fails if anything that previously passed no longer
does, or if a count goes down. Raising the baseline is part of the change that
earns it.
