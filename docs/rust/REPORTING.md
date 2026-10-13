# Reporting a problem or a slow spot

You are using hydrus-rs on a real collection, with the Python client as a
fallback. Two things make a problem cheap to diagnose: the crash log, which
is always on, and `hydrus bench`, which measures how fast things are on
*your* collection. This page says how to use both, and what to put in a bug
report so that a coding session can turn it into a regression test against
the reference.

## Timing your collection: `hydrus bench`

```sh
hydrus bench /path/to/store > bench.txt
hydrus bench /path/to/store --json > bench.json     # the same, for tools
```

On Windows: `hydrus.exe bench D:\path\to\store`. It needs only the binary
(`target/release/hydrus`, or `hydrus.exe`). Options:

- `--runs N`: runs of each measurement (default 5).
- `--search "blue sky; system:inbox"`: time a search of your own as well, as
  tags separated by `;`. Give it several times for several searches. Use the
  searches that feel slow.
- `--thumbnails N`: how many thumbnails to read and decode (default 200).

It is **strictly read-only**: the store is opened with SQLite's read-only
flag, nothing is migrated, created or regenerated, and nothing is written, not
even a crash log. It is safe on your daily store, with the client open or
closed. If the store needs a schema migration it says so and stops (open it
with the client once). Close other heavy programs first, and run it twice:
the first run warms the operating system's file cache, and the report shows
the *first* run of each measurement as well as the median and the 90th
percentile, so a cold start is visible too.

Send the whole output (paste the text; attach the JSON if asked). It holds
the version, the machine (operating system, CPU, memory), the size of the
store (files, tags, mappings, potential pairs, database size) and each
timing in milliseconds with a note of what it found, so a fast time on an
empty result is not mistaken for a fast search. It contains no tag names
other than the popular tags it searches for and the `--search` text you give
it; read it before you paste if that matters to you.

What is measured is listed in `docs/rust/BENCHMARKS.md`.

## When it crashes: the crash log

Every Rust panic, on any thread, in the desktop client, `hydrus serve` and
the other `hydrus` commands, is written to

    <store directory>/client crash.log

(the directory you gave `hydrus-gui` or `hydrus serve`; the same place as
`client.db`). Each entry has the time (UTC), the version (crate version and
git commit), the program and thread, the panic message and where it
happened, and a backtrace. The program also prints a line on stderr naming
the log. The log is capped: past 512 KiB it moves to `client crash.log.old`
and starts again, so there are never more than those two files.
Help > debug > debug modes > "use faulthandler to log crashes" is shown on
and choosing it tells you where the log is.

Limits: a panic is logged, but a crash that is not a panic (the operating
system killing the program for memory, a segfault inside a library such as
libmpv or ffmpeg) is not; for those, say what you were doing and look at
your operating system's crash or kernel log. A panic in a background thread
often does not close the program: it can leave one feature quietly stopped
(a popup that never finishes, a worker that stops), so check the log when
something silently stops working, not only when the program closes.

## What to put in a bug report

1. **The version.** `hydrus --version`, plus the line `version:` from the
   top of `hydrus bench` or from a crash log entry (it carries the git
   commit).
2. **Steps.** The exact clicks or commands, from a fresh start of the
   client, as few as you can. Name the page, the dialog and the menu entry
   as they appear on screen.
3. **Expected vs actual.** Say what the Python client does at the same step
   (you still have it): "hydrus shows 12 files, hydrus-rs shows 11".
   Unusual-but-correct behaviour in the reference is the most useful kind of
   report, because the reference is what hydrus-rs is tested against. Check
   `docs/rust/DIFFERENCES.md` first: a difference listed there is known.
4. **The crash log** if anything crashed or stopped: the newest entry of
   `client crash.log`, in full, with its backtrace.
5. **A bench report** for anything slow, or the time you saw ("about 8
   seconds to open the first page") and the search.
6. **Your data**, only as needed: a tag or search text that triggers it, a
   hash of a file that misbehaves (the sha256 in its file info), and
   whether the files came from the import or were added later. Do not send
   the database.

## How a session turns a report into a regression test

For whoever picks the report up (a coding session works from `AGENTS.md`):

1. Find the leaf the behaviour belongs to: `python3 scripts/track.py show
   <leaf-id>`, and the code in `hydrus/` that does the thing in the report.
2. **Reproduce it in the reference.** Write a recorder in `oracle/`
   (`oracle/README.md`; copy a recent one such as `record_frame_placement.py`
   or `record_subscriptions_list.py`): `hydrus_driver.run_client` on the
   `basic` fixture, or a fixture built to hold the reporter's situation
   (the tag, the search text, the file properties from the report, never
   the reporter's data), replace dialogs with scripted answers, hold time
   still. It writes a fixture into `oracle/fixtures/`. Commit the script
   and the fixture together.
3. **Show hydrus-rs disagrees.** A test in
   `crates/hydrus-gui-model/tests/model/<area>.rs` (or
   `crates/hydrus-gui/tests/gui/<area>.rs` for a window), driven as a user
   would and compared step by step with the recording. It fails first.
4. **Fix it** until it passes, and tag the test `// leaf: <leaf-id>` only
   if it proves what the leaf names (`AGENTS.md`, "Tag honestly").
5. For a crash: the backtrace names the function. Reproduce it with the
   input from the report in a test of that function first, then at the
   level the user hit it. The crash log's version line says which commit to
   check out. If the panic came from code with no reference counterpart
   (hydrus-rs's own plumbing), a plain regression test with the report's
   input is the proof.
6. For a slow spot: the bench report gives the baseline. Reproduce with
   `hydrus bench` on `hydrus_store::synth` data of a similar size (a
   store of the reporter's shape, not their data), or with the reference's
   timing for the same step from `oracle/bench_api.py`, and put the number
   in `docs/rust/BENCHMARKS.md` with the change.
