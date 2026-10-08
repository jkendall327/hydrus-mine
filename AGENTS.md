# Working on hydrus-rs

This repository holds two things: **hydrus**, the Python client in `hydrus/`
(the *reference*), and **hydrus-rs**, a Rust port of it in `crates/`. Work here
is porting the reference's behaviour to hydrus-rs, proven against the
reference by recorded oracles. These notes are for coding agents (Codex reads
`AGENTS.md`, Claude Code reads `CLAUDE.md`, which points here) and for the
humans directing them.

Read first:

1. `docs/rust/ARCHITECTURE.md`: the crates and how they fit.
2. `docs/rust/CONVENTIONS.md`: the rules every crate follows.
3. `docs/rust/tracking/README.md`: what is left, grouped into workstreams, and
   how a leaf counts as done.
4. `docs/rust/GUI.md` (what the GUI does) and `docs/rust/DIFFERENCES.md`
   (where hydrus-rs differs from the reference), for the area you work on.
5. `oracle/README.md`: running the reference to record ground truth.

## Goal

Every feature leaf of the reference GUI ported and tested against reference
recordings, Linux first. Windows and macOS are deferred; do not spend time on
them unless asked.

## The loop

Keep slices small (one dialog's list, one button's questions) and keep moving.
Verification is a fast local loop plus CI on every push, not a publication
ceremony.

1. **Pick work** from your workstream in `docs/rust/tracking/`.
2. **Read the reference.** `grep -rn` in `hydrus/` (GUI code in
   `hydrus/client/gui/`). Note every string the user sees, every question
   asked, and what each button does.
3. **Record it** when behaviour is non-trivial: a recorder in `oracle/` drives
   the real reference client and writes a fixture to `oracle/fixtures/`. Copy a
   recent one (`oracle/record_downloader_lists.py`,
   `oracle/record_subscriptions_list.py`, `oracle/record_sessions_menu.py`):
   `hydrus_driver.run_client` on the `basic` fixture, work on the Qt thread with
   `controller.CallBlockingToQt`, replace dialogs with scripted answers, hold
   time still. Run with
   `QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_x.py`. Commit the
   script and fixture together.
4. **Port it.** Logic without Slint goes in `crates/hydrus-gui-model` (fast to
   build and test). Slint files are `crates/hydrus-gui/ui/*.slint`, wired in
   `crates/hydrus-gui/src/<area>_window.rs` (or `lib.rs` for the main window).
   Lists use `ui/list_table.slint`'s `ListTable` with `list_selection`.
5. **Test it against the recording.** Model tests in
   `crates/hydrus-gui-model/tests/model/<area>.rs`; GUI tests (headless Slint,
   a real store) in `crates/hydrus-gui/tests/gui/<area>.rs`. Replay the
   recording step by step. Cover cancellation, invalid input, persistence
   after reopening, and the consumers of edited settings.
   **Tag** each test that proves a leaf: a comment `// leaf: <leaf-id>` on the
   line(s) just above the `#[test]`. That tag is how the tracker counts the
   leaf as done.
6. **Run the local loop** (`scripts/dev.sh`, see below) until it is green.
7. **Write it down** in the same commit: `docs/rust/GUI.md` says what works in
   the user's terms; `docs/rust/DIFFERENCES.md` says what is missing or
   different from the reference, and why.
8. **Commit and push.** CI checks the push. Do not wait for it: start the next
   slice. If CI fails, fix it in your next commit.

### Done means

A leaf is done when a test tagged `// leaf: <id>` exercises its behaviour
against the reference (a recording, or the reference's own strings and
logic where no recording is needed) and passes on master. Nothing else:
no ledgers, evidence packets, census files or screenshots in git. Be honest
in tags: tag a leaf only when the test covers what the leaf names. If a leaf
is only partly done, say what is missing in `DIFFERENCES.md` and do not tag it.

### Review

Each pull request (a batch of slices from one workstream, typically 5-20
leaves) gets one independent review before merge: a reviewer reads the diff
against the reference code and recordings, checks the tags are honest, and
looks at the GUI renders CI uploads (`gui-renders-*` artifact) for windows
that changed. Findings are fixed in the same PR.

## The local loop: `scripts/dev.sh`

The GUI compiles locally; use it. Measured on a 4-core, 16 GB machine:

| Command | What | Time |
|---|---|---|
| `scripts/dev.sh slint` | compile the `.slint` files only | ~10 s |
| `scripts/dev.sh gui export_files::` | GUI tests matching a filter | ~20 s after a Rust edit |
| `scripts/dev.sh model export_files` | model tests matching a filter | ~25 s after a model edit |
| `scripts/dev.sh lint` | rustfmt + strict Clippy on changed crates | ~15-25 s |
| `scripts/dev.sh pre-push` | lint, slint check, tests of changed crates | a few minutes |
| `scripts/dev.sh ui` | the generated UI crate alone | ~7 min after a `.slint` edit |

- **Run `scripts/dev.sh slint` after every `.slint` edit** before anything
  else; it reports Slint errors in seconds.
- **Batch `.slint` edits.** Any `.slint` change rebuilds the 87 MB generated
  crate (~7 min, ~13 GB). Make all the UI changes a slice needs, check them
  with `dev.sh slint`, then build once.
- **The first build in a fresh checkout is slow** (~13 min dependencies,
  ~7 min UI crate, ~35 min for the first Clippy run). Keep a checkout's
  `target/` across sessions when you can.
- The generated UI crate (`generated/hydrus-gui-ui`, deliberately outside the
  workspace members so Clippy never lints its generated code) is built on its
  own first because rustc needs ~13 GB for it; Cargo would otherwise start the GUI crate alongside it and run out of
  memory. `scripts/dev.sh` does this for you. If you call Cargo directly, run
  `cargo build -p hydrus-gui-ui` before building or testing `hydrus-gui`.
- The `hydrus-workspace-hack` crate (managed by `cargo hakari`) unifies
  features so any `-p` selection reuses the same dependency builds. After
  adding or changing a dependency, run `cargo hakari generate && cargo hakari
  manage-deps`. CI fails if it is stale.

## CI

`.github/workflows/rust.yml` runs on every push to `master`, `claude/**` and
`codex/**`, and on pull requests: a Slint check, a lint job (fmt, hakari,
strict Clippy, parity ratchet) and a test job (all Linux tests including the
715+ GUI tests), in parallel. A newer push cancels the older run. Windows and
macOS run only when dispatched by hand with `secondary_platforms`.

## Several agents at once

- Each agent takes one workstream from `docs/rust/tracking/` and works in its
  own branch or worktree. Workstreams are drawn so their files rarely overlap.
- Shared hot spots: `crates/hydrus-gui/src/lib.rs`,
  `crates/hydrus-gui/ui/main.slint`, `crates/hydrus-gui-model/src/lib.rs`,
  `crates/hydrus-gui/tests/gui/main.rs`, `crates/hydrus-gui-model/tests/model/main.rs`,
  `docs/rust/GUI.md`, `docs/rust/DIFFERENCES.md`. Keep edits there to a line
  or a paragraph (a module line, one callback's wiring) so merges stay easy;
  put the bulk of a feature in its own files.
- **Memory:** only one generated-UI build fits on a 16 GB machine at a time.
  Agents sharing a machine should share one `target/` directory (Cargo's lock
  serializes builds) or stagger `.slint` changes. Model-crate work needs no
  UI build at all.

## Commits

- Small and self-contained, each passing `scripts/dev.sh pre-push`.
- Subject in the imperative, saying what the user gets, often "..., as the
  reference does". The body says what changed and which recording proves it.

## Things that have bitten before

- **Slint**: a `LineEdit` the user typed in drops its one-way `text`
  binding; use `<=>`, or replace the model so the element is made anew.
  A `Window`'s `title` can't be set from Rust; give the window its own
  `window-title` property. `StandardTableView` selects one row only; use
  `ListTable`. Standard widgets such as `Button` cannot have children
  (a `Timer` inside a `Button` fails to compile).
- **Slint geometry**: a child-to-parent two-way link keeps the parent's
  existing binding or value. An unbound parent output can therefore replace
  measured child geometry with its default zero. Expose measured geometry
  through read-only direct bindings to structurally present items.
- **Resizable Slint windows**: use `preferred-width`/`preferred-height` for the
  opening size. A literal `width` binding can constant-fold descendant viewport
  measurements even after the native window resizes.
- **Winit event filters**: `on_winit_window_event` replaces the prior callback.
  Compose opening observers and existing drop handlers in one owner-local filter.
- **Key handling**: the main window's shortcuts are a `capture-key-pressed` on
  the outermost `FocusScope`, so they work whatever has focus; a widget's own
  keys go in its own `FocusScope`.
- **Timers in GUI tests**: state updated by a Slint `Timer` (refresh ticks,
  delayed enables) is not there until the timer runs; spin the event loop
  until it is, rather than asserting immediately.
- **Time**: anything that shows "5 minutes ago" takes `now` as an argument, so
  tests and recordings can hold it still.
- **The reference client runs its own worker threads.** In recorders, pause the
  importers you make and give the client a moment between steps that need it.
- **Disk**: `target/` grows past 20 GB. Delete `target/debug/incremental` when
  space runs low.
- **Watch loops**: `pgrep -f <pattern>` (and `pkill -f`) match the calling
  shell's own command line. Wait on an output file instead.
