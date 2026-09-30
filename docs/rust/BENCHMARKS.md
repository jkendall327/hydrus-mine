# Benchmarks

Measured in the development container (4 cores, SSD-backed, release builds),
with SQLite `synchronous=FULL`: every write is durable before it returns.

## The target install

Counts from the owner's main hydrus install (v682), which sizes what has
to be fast:

| what | count |
|---|---|
| files ever imported | 746,437 |
| files with a perceptual hash / distinct hashes | 441,707 / 414,569 |
| files searched for similar files at distance 2 / 4 / not yet | 174,528 / 101,504 / 50,084 |
| potential duplicate pairs (distance 0 / 1-2 / over 2, max 4) | 23,344 (12,648 / 8,831 / 1,865) |
| duplicate groups | 385,346 |
| duplicates auto-resolution rules | 5 |
| tags / URLs | 542,985 / 1,918,810 |
| subscriptions / their queries' histories | 14 / 344 (117 MB serialised) |
| import folders / export folders | 1 / 1 |

Searches for similar files are mostly at distance 0 and 2.

## Similar-files search

`crates/hydrus-store/src/similar.rs` (`scale`, release build): 414,569
perceptual hashes (the target install's count), 50,000 files searched.

| distance | index built | 50,000 searches |
|---|---|---|
| 2 | 0.2 s | 0.04 s |
| 4 | 0.1 s | 0.7 s |
| 8 | (none: every hash is checked) | 37 s |

So the target install's 50,000 unsearched files take about a second at
distance 2 or 4, plus the database work of recording what they find.

## Store micro-benchmarks

`cargo bench -p hydrus-store` (criterion) runs against a synthetic library
of 20,000 files, 5,000 tags, 20 tags per file and 200 sibling and 200 parent
pairs, built through the normal write path (`hydrus_store::synth`).

| operation | time |
|---|---|
| tag 1,000 files with a popular tag, then untag them (two commits) | 85 ms |
| add 30 tags to one file, then remove them (one commit) | 4.4 ms |
| load full metadata for 100 files (`/get_files/file_metadata`) | 2.8 ms |
| rebuild every autocomplete count from scratch (400k mappings) | 1.4 s |

Every write above also keeps storage and display autocomplete counts current,
across every file domain.

## Write throughput

`cargo run --release -p hydrus-store --example synth_throughput -- 100000 20`
generates 100,000 files and 2,000,000 mappings, in batches of 1,000 files per
transaction:

| | |
|---|---|
| files | 1,400 per second |
| mappings, with counts maintained | 28,000 per second |
| full count rebuild afterwards | 14 s |
| database size | 86 MiB |

Per-tag statement overhead dominates here, because most tags in a batch touch
only a few files. Tag repository processing (millions of mappings per
update) will get a set-based bulk path.

## Against the reference client

`oracle/bench_api.py` times the same Client API requests, one at a time,
against the reference client and against hydrus-rs (release build) serving
the same library imported in place. The library is built in the reference
by `oracle/make_bench_db.py` (records only, no media; skewed tag popularity,
namespaces, siblings and parents). Medians of five runs after a warm-up.

50,000 files, about 1,000,000 mappings (the reference wrote it at 343 files
and 6,869 mappings a second; measured before hydrus-rs cached file domains
between searches, see below):

| request | reference | hydrus-rs | speed-up |
|---|---|---|---|
| get_services | 1.7 ms | 0.6 ms | 3x |
| file_metadata, 1 file | 4.3 ms | 2.0 ms | 2x |
| file_metadata, 100 files | 47.5 ms | 17.8 ms | 3x |
| file_metadata, 100 files, basic | 6.0 ms | 2.1 ms | 3x |
| search_tags "tag number 1" | 56.2 ms | 26.4 ms | 2x |
| search_tags "character:*" | 1.1 ms | 0.7 ms | 2x |
| search_files, popular tag (29,000 files) | 72.0 ms | 29.7 ms | 2x |
| search_files, 2 tags | 12.0 ms | 6.5 ms | 2x |
| search_files, tag + system | 74.6 ms | 18.3 ms | 4x |
| search_files, everything | 114.5 ms | 38.1 ms | 3x |
| add_tags, 1 file, 5 tags | 4.6 ms | 1.0 ms | 4x |
| add_tags, 100 files, 1 tag | 5.4 ms | 1.3 ms | 4x |

400,000 files, about 8,000,000 mappings, with duplicates at the target
install's scale added by `oracle/add_bench_duplicates.py`: every file in a
duplicate group, 100,000 of them in groups of two or three, and 23,344
potential pairs (12,648 at distance 0, 8,831 at 1-2, 1,865 at 3-4). The
reference wrote the library at 171 files a second.

| request | reference | hydrus-rs | speed-up |
|---|---|---|---|
| get_services | 2.0 ms | 0.7 ms | 3x |
| file_metadata, 1 file | 4.9 ms | 2.0 ms | 2x |
| file_metadata, 100 files | 51.0 ms | 19.2 ms | 3x |
| file_metadata, 100 files, basic | 6.5 ms | 2.2 ms | 3x |
| search_tags "tag number 1" | 374.0 ms | 113.8 ms | 3x |
| search_tags "character:*" | 1.7 ms | 0.7 ms | 3x |
| search_files, popular tag (141,000 files) | 392.5 ms | 30.1 ms | 13x |
| search_files, 2 tags | 39.5 ms | 10.4 ms | 4x |
| search_files, tag + system | 383.6 ms | 33.2 ms | 12x |
| search_files, everything | 983.1 ms | 31.6 ms | 31x |
| add_tags, 1 file, 5 tags | 4.4 ms | 1.3 ms | 3x |
| add_tags, 100 files, 1 tag | 6.4 ms | 1.4 ms | 4x |
| potentials count | 203.8 ms | 6.9 ms | 30x |
| potentials count, distance 0 | 157.1 ms | 5.7 ms | 27x |
| potentials count, popular tag | 438.6 ms | 32.8 ms | 13x |
| potential pairs, batch of 50 | 58.7 ms | 13.8 ms | 4x |
| potential pairs, group mode | 7267.6 ms | 13.6 ms | 533x |
| random potentials | 329.4 ms | 37.4 ms | 9x |
| file_relationships, 100 files | 13.5 ms | 11.7 ms | 1x |

These are warm figures: each request is repeated, and hydrus-rs keeps each
file domain's files, its import order and the potential pairs in memory
until a write changes them (see STORE.md). The first search of a domain
after a write loads it again, which at 400,000 files costs about 30 ms for
its files and 50 ms for its import order; loading the potential pairs costs
about 70 ms. A popular-tag search with nothing cached takes 113 ms.

