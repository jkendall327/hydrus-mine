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

To come once `/get_files/search_files` lands: the same synthetic library
built in the reference client, imported, and the same Client API request mix
timed against both.
