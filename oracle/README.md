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

## The driver

`hydrus_driver.py` boots the unmodified reference client **in-process** with an
offscreen Qt platform, then runs a hook on a worker thread with the live
controller and the client's own Client API (enabled on a local port with fixed
oracle access keys). When the hook returns, the client shuts down through its
normal SIGTERM path and the driver waits for the database to close. One boot
per process; use `run_in_subprocess` for more.

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
