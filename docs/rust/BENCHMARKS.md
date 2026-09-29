# Benchmarks

Measured in the development container (4 cores, SSD-backed, release builds),
with SQLite `synchronous=FULL`: every write is durable before it returns.

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
