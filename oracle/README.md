# oracle

Tooling that runs the **reference implementation** (the Python hydrus client
in `hydrus/`) to record ground truth for the Rust implementation's tests.

Nothing in the Rust test suite runs Python: the outputs of these scripts are
committed under `oracle/fixtures/` and `oracle/recordings/`. Re-running a
script and committing a changed fixture is a deliberate, reviewable act.

## Setup

```sh
python3 -m venv ~/pyenv
~/pyenv/bin/pip install -r oracle/requirements.txt
# the reference client needs these at runtime, even headless:
sudo apt-get install -y libegl1 libgl1 libxkbcommon0 libfontconfig1 libdbus-1-3 ffmpeg
export QT_QPA_PLATFORM=offscreen
```

## Scripts

| script | output |
|---|---|
| `dump_constants.py` | `fixtures/constants.json`: file types, service types, enum codes |
| `dump_tag_cleaning.py` | `fixtures/tag_cleaning.json`: tag cleaning on awkward inputs |
| `dump_system_predicates.py` | `fixtures/system_predicates.json`: search predicate parsing (system predicates and Client API tag lists) over a large corpus |
| `make_import_media.py` | `fixtures/import_media/`: small deterministic media corpus (committed) |
| `make_fixture_db.py` | `fixtures/legacy_db/<name>.tar.gz` + `.manifest.json`: a populated reference database |
| `dump_legacy_expectations.py` | `fixtures/legacy_db/<name>.expected.json`: what the reference reads from a fixture database, for `hydrus-legacy`'s tests |
| `build_scenarios.py` | `scenarios/*.json`: Client API conformance scenarios (declarative request lists) |
| `record_api.py` | `recordings/*.json`: the reference client's responses to each scenario |
| `dump_duplicate_merges.py` | `fixtures/duplicate_merges.json`: what duplicate decisions with metadata merges leave in the reference's database |
| `dump_note_merges.py` | `fixtures/note_merges.json`: note merging ("merge cleverly") on duplicate decisions |
| `dump_media.py` | `fixtures/media.json`: a generated media corpus and the reference's view of it |
| `dump_domains.py` | `fixtures/domains.json`: domain helpers over a public-suffix corpus |
| `dump_url_classes.py` | `fixtures/url_classes.json`: URL classes making referral URLs and next pages |
| `dump_gugs.py` | `fixtures/gugs.json`: gallery URL generators on random searches |
| `dump_formulas.py` | `fixtures/formulas.json`: parsing formulas on random documents |
| `dump_page_parsers.py` | `fixtures/page_parsers.json`: page parsers on random documents |
| `dump_string_processing.py` | `fixtures/string_processing.json`: string processors on random lists |
| `dump_cookie_jars.py` | `fixtures/cookie_jars.json`: cookie jars as the reference pickles them |
| `dump_import_options.py` | `fixtures/import_options.json`: import option defaults and layering |
| `dump_subscriptions.py` | `fixtures/subscriptions.json`: subscriptions and their queries' history as stored |
| `dump_sidecars.py` | `fixtures/sidecars.json`: sidecar routing between sidecar files |
| `dump_auto_resolution.py` | `fixtures/auto_resolution.json`: duplicates auto-resolution rules as stored |
| `dump_visual_data.py` | `fixtures/visual_data.json`: the visual-duplicates computations, stage by stage |
| `dump_casefold.py` | `crates/hydrus-core/src/casefold_table.rs`: Python's `str.casefold`, as a table |
| `record_media_tests.py` | `fixtures/media_tests.json`: in-memory predicate tests on the `basic` fixture's files |
| `record_url_class_search.py` | `fixtures/url_class_search.json`: searches by URL class, through the Client API and in memory |
| `record_similar_files.py` | `fixtures/similar_files.json`: the similar-files search on generated near-duplicates |
| `record_auto_resolution.py` | `fixtures/auto_resolution_run.json` + `legacy_db/auto_resolution.tar.gz`: auto-resolution rules run on generated files |
| `record_downloads.py` | `fixtures/downloads.json`: the downloader against a local fake site |
| `record_import_folder.py` | `fixtures/import_folder_run.json` + `legacy_db/import_folder.tar.gz`: an import folder run |
| `record_export_folder.py` | `fixtures/export_folder_run.json` + `legacy_db/export_folder.tar.gz`: export folder runs |
| `make_bench_db.py` | a large synthetic library in the reference, for benchmarks |
| `bench_api.py` | timings of a Client API request mix against the reference and hydrus-rs |

## The driver

`hydrus_driver.py` boots the unmodified reference client **in-process** with an
offscreen Qt platform, then runs a hook on a worker thread with the live
controller and the client's own Client API (enabled on a local port with fixed
oracle access keys). When the hook returns, the client shuts down through its
normal SIGTERM path and the driver waits for the database to close. One boot
per process; use `run_in_subprocess` for more.

To keep recordings reproducible the driver turns off the reference's
background similar-files search (it adds potential pairs at unpredictable
moments) and pins the member it picks at random to stand for a duplicate
group whose king is out of view to the lowest file id, as hydrus-rs does.

Fixture databases are populated through the same paths real use takes: the
Client API wherever possible, and controller `Write` commands for the rest
(services, siblings/parents, display application, similar-file search).

## Conformance scenarios

A scenario is a list of HTTP requests against a fixture database. Every
scenario is recorded on a fresh boot of a fresh copy of its fixture: even
"read-only" requests can have side effects in the reference (asking for an
unknown hash's thumbnail assigns it a file id), so sharing boots would make
recordings depend on scenario order. The Rust conformance runner
(`crates/hydrus-api/tests/conformance.rs`) replays the same JSON against an
imported copy of the same fixture and diffs the responses.

Request placeholders: `{MEDIA}` becomes the absolute path of
`fixtures/import_media`. Response normalisation: the recording db dir's
absolute path becomes `{DB_DIR}`, and `{MEDIA}` likewise. Binary bodies are
recorded as sha256 + length.

Where the reference is wrong (a bug users would not rely on), the Rust side
does not copy it: the difference is recorded in `docs/rust/DIFFERENCES.md`.
