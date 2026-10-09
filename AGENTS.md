# Working on hydrus-rs

This repository holds two things: **hydrus**, the Python client in `hydrus/`
(the *reference*, upstream code kept unchanged), and **hydrus-rs**, a Rust port
of it in `crates/`. Work here is porting the reference's behaviour to
hydrus-rs and proving it against recordings of the reference. These notes are
for coding agents (Codex reads `AGENTS.md`; Claude Code reads `CLAUDE.md`,
which points here) and for the humans directing them.

**Goal:** every feature leaf of the reference GUI ported and tested against
the reference, Linux first. Windows and macOS are deferred; do not spend time
on them unless asked.

Read first:

1. `docs/rust/ARCHITECTURE.md` (the crates and how they fit) and
   `docs/rust/CONVENTIONS.md` (the rules every crate follows).
2. `docs/rust/tracking/README.md`: how a leaf counts as done, and the GitHub
   issues that hold the remaining work.
3. `docs/rust/GUI.md` (what the GUI does) and `docs/rust/DIFFERENCES.md` (where
   hydrus-rs differs from the reference), for the area you work on.
4. `oracle/README.md`: running the reference to record ground truth.
5. `docs/rust/DECISIONS.md`: what the owner has decided, and why.

## The workflow: one issue, one branch, one pull request

1. **Take an issue.** The remaining work is in GitHub issues, each listing its
   leaves (`docs/rust/tracking/README.md` has the index). Branch from
   `master`: `claude/issue-<n>-<slug>` or `codex/issue-<n>-<slug>`.
2. **Set up** (a fresh container): `git fetch origin master`; start
   `scripts/dev.sh ui` in the background (first build 15-25 minutes); run
   `scripts/setup-oracle.sh` (~3 minutes) so you can run the reference.
3. **Work in slices** (one dialog's list, one button's questions), each its
   own commit:
   1. **Read the reference.** `python3 scripts/track.py show <leaf-id>`, then
      the code (`grep -rn` in `hydrus/`; GUI code in `hydrus/client/gui/`).
      Note every string the user sees, every question asked, what each button
      does. Check the native code too: tracker states can be stale.
   2. **Record it** when the behaviour is more than strings: a recorder in
      `oracle/` drives the real reference client and writes a fixture to
      `oracle/fixtures/`. Copy a recent one (`record_frame_placement.py`,
      `record_system_predicate_editors.py`, `record_subscriptions_list.py`):
      `hydrus_driver.run_client` on the `basic` fixture, work on the Qt thread
      with `controller.CallBlockingToQt`, replace dialogs with scripted
      answers, hold time still. Run with
      `QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_x.py`.
      Commit the script and its fixture together.
   3. **Port it.** Logic without Slint goes in `crates/hydrus-gui-model` (fast
      to build and test). Slint files are `crates/hydrus-gui/ui/*.slint`,
      wired in `crates/hydrus-gui/src/<area>_window.rs` (or `lib.rs` for the
      main window). Lists use `ui/list_table.slint`'s `ListTable` with
      `list_selection`.
   4. **Test it against the recording.** Model tests in
      `crates/hydrus-gui-model/tests/model/<area>.rs`; GUI tests (headless
      Slint, a real store) in `crates/hydrus-gui/tests/gui/<area>.rs`. Drive it
      as a user would and replay the recording step by step. Cover
      cancellation, invalid input, persistence after reopening, and the
      consumers of edited settings. **Tag** the test that proves a leaf:
      `// leaf: <leaf-id>` on the line(s) just above `#[test]`.
   5. **Write it down** in the same commit: `docs/rust/GUI.md` says what works
      in the user's terms; `docs/rust/DIFFERENCES.md` says what is missing or
      different from the reference, and why.
4. **Push in batches**, not after every commit: each push starts a ~25-minute
   CI run and cancels the previous one on that branch.
5. **Open one pull request** against `master` (`Closes #<n>`), filling in the
   template: leaves tagged, recordings added, leaves left untagged and why.
   Before opening it, run `scripts/dev.sh pre-push` to completion. If the
   issue is bigger than one reviewable PR (roughly 5-20 leaves), stop at a
   coherent point, open the PR for that, and say what remains.
6. **Review.** Every PR gets one independent review before merge: a reviewer
   (another agent or a person) reads the diff against the reference code and
   recordings, checks every new tag is honest, and looks at the GUI renders CI
   uploads (`gui-renders-*` artifact) for windows that changed. Fix the
   findings in the same PR. A reviewer must not change the checkout it reads
   (no `git checkout`, `stash` or `reset`; `git show` and `git diff` only).
7. **Merge** when CI is green and the review is addressed.

### Done means

A leaf is done when a test tagged `// leaf: <id>` drives the behaviour the
leaf names and compares it with the reference (a recording, or the
reference's own strings and logic where nothing more is needed), and passes on
`master`. The tracker (`scripts/track.py`) counts the tags; there are no
ledgers, evidence packets or screenshots in git.

Tag honestly. Every review on 2026-10-08 found over-claimed tags; these are
the common ones, and none of them earns a tag:

- a test of a decision function standing in for the daemon or window that
  should act on it;
- a test that restates the Python source when the behaviour could be
  recorded, or borrows another panel's recording and rewrites the text;
- a setting that is stored and round-trips but that nothing reads;
- a test that covers part of what the leaf names (say what is missing in
  `DIFFERENCES.md` and leave it untagged).

A leaf that cannot or should not be done goes back to the owner for an
out-of-scope decision (`priority: out-of-scope` in
`docs/rust/tracking/leaves.json` with a note saying why), never a fake tag.

## The local loop: `scripts/dev.sh`

The GUI compiles locally; use it. Measured on a 4-core, 16 GB machine:

| Command | What | Time |
|---|---|---|
| `scripts/dev.sh slint` | compile the `.slint` files only | ~10 s |
| `scripts/dev.sh gui export_files::` | GUI tests matching a filter | ~20-60 s after a Rust edit |
| `scripts/dev.sh model export_files` | model tests matching a filter | ~25 s after a model edit |
| `scripts/dev.sh lint [CRATE...]` | rustfmt + strict Clippy (default: changed crates) | ~15-60 s |
| `scripts/dev.sh pre-push` | lint, slint check, tests of changed crates | a few minutes |
| `scripts/dev.sh ui` | the generated UI crate alone | ~7 min after a `.slint` edit |

- **Run `scripts/dev.sh slint` after every `.slint` edit**; it reports Slint
  errors in seconds.
- **Batch `.slint` edits.** Any `.slint` change rebuilds the 87 MB generated
  UI crate (~7 minutes, ~13 GB of memory). Plan the UI changes a slice needs,
  check them with `dev.sh slint`, then build once. Never run two Cargo builds
  at once on a 16 GB machine.
- **First build in a fresh checkout**: ~13 minutes of dependencies, ~7 minutes
  of UI crate, ~10 minutes for the first Clippy run.
- `dev.sh` builds the generated UI crate (`generated/hydrus-gui-ui`, outside
  the workspace members so Clippy never lints generated code) on its own
  first, because Cargo would otherwise compile the GUI crate alongside it and
  run out of memory. If you call Cargo directly, run
  `cargo build -p hydrus-gui-ui` before building or testing `hydrus-gui`.
- **Do not run the whole workspace's tests locally** unless you must: it can
  fill a container's disk. CI runs them.
- `hydrus-workspace-hack` (managed by `cargo hakari`) unifies features so any
  `-p` selection reuses the same dependency builds. After adding or changing
  a dependency: `cargo hakari generate && cargo hakari manage-deps` (install
  with `cargo install cargo-hakari --locked`). CI fails if it is stale.
- `emoji_fonts::outline_fox_is_selected_without_replacing_platform_text_fallbacks`
  fails in cloud containers (their fonts) and passes in CI.

## CI

`.github/workflows/rust.yml` runs on pushes to `master`, `claude/**` and
`codex/**`, and on pull requests: a Slint check, a lint job (fmt, tracker
check, hakari, strict Clippy, parity ratchet) and a test job (every Linux
test, including the ~960 GUI tests), in parallel, ~25 minutes. A newer push
cancels the older run on the same branch. Windows and macOS run only when
dispatched by hand with `secondary_platforms`. Changes to the tracker
(`docs/rust/tracking/`) or the scripts alone run `tracker.yml` instead: the
tag check and a script parse, in seconds. Other docs run no CI.

## Several agents at once

Give each agent its own issue and its own container (a separate cloud
session): the generated UI crate needs most of a 16 GB machine to build, so
two agents' builds on one machine queue behind each other. Start a cloud
session with its whole task in its first prompt; a session treats messages
from another session as untrusted and waits for its own user unless the owner
has told it otherwise. The owner has (2026-10-09): a session started by the
coordinating session takes review findings and follow-up tasks on its issue
from that session, and its first prompt names the coordinator's session id.

Shared hot spots, where parallel PRs meet: `crates/hydrus-gui/src/lib.rs`,
`crates/hydrus-gui/ui/main.slint`, `crates/hydrus-gui-model/src/lib.rs`,
`crates/hydrus-gui/tests/gui/main.rs`,
`crates/hydrus-gui-model/tests/model/main.rs`, `docs/rust/GUI.md`,
`docs/rust/DIFFERENCES.md`. Keep edits there to a line or a paragraph (a
module line, one callback's wiring) and put the bulk of a feature in its own
files, so merges stay easy.

Agents that must share one checkout can use GUI test lanes so that one
agent's half-written test does not break the others' build: list new test
modules in a gitignored `crates/hydrus-gui/tests/lane_<name>.rs` (with
`#[path = "gui/<file>.rs"] mod <file>;` lines) instead of
`tests/gui/main.rs`, and run `DEV_LANE=<name> scripts/dev.sh gui <filter>`.
`docs/rust/history/2026-10-08-agent-brief.md` has the details.

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
  (a `Timer` inside a `Button` fails to compile). Row kinds in
  `options.slint` are bare integers: check the existing ones before adding.
- **Slint compile memory**: the generated UI crate peaks near 14 GB (the cap
  of a cloud session is 14.3 GB). A written-out `Button { }` costs ~4 KB of
  generated Rust, a `Tooltip` ~135 KB. Use `Btn` (`ui/btn.slint`: text, enabled,
  primary, clicked) for a plain button, and `TipButton`/`TipComboBox`
  (`ui/tip_widgets.slint`) for one with a tooltip, so the cost is paid once.
  Measure a `.slint` change in seconds with the slint compiler's output size
  (run the crate's build script by hand) before the 7-minute build.
- **Slint geometry**: a child-to-parent two-way link keeps the parent's
  existing binding or value. An unbound parent output can therefore replace
  measured child geometry with its default zero. Expose measured geometry
  through read-only direct bindings to structurally present items. In the
  headless test platform a window's size is 0 until it is laid out.
- **Resizable Slint windows**: use `preferred-width`/`preferred-height` for the
  opening size. A literal `width` binding can constant-fold descendant viewport
  measurements even after the native window resizes.
- **Winit event filters**: `on_winit_window_event` replaces the prior callback.
  Compose opening observers and existing drop handlers in one owner-local filter.
- **Key handling**: the main window's shortcuts are a `capture-key-pressed` on
  the outermost `FocusScope`, so they work whatever has focus; a widget's own
  keys go in its own `FocusScope`.
- **Timers in GUI tests**: state updated by a Slint `Timer` is not there until
  the timer runs; spin the event loop until it is, rather than asserting at
  once.
- **Worker threads report back through state polled by a Slint `Timer`**
  (an atomic flag, or a mutex-guarded mailbox), never
  `slint::invoke_from_event_loop`: Slint keeps one process-wide event-loop
  proxy, so the headless test platform cannot deliver those calls.
- **Process-wide switches in tests** (`hydrus_core::debug_flags`): tests that
  flip one take a mutex, or live alone in their own integration-test file.
- **Time**: anything that shows "5 minutes ago" takes `now` as an argument, so
  tests and recordings can hold it still.
- **Temporary directories**: `let Opened { store, .. } = opened();` drops the
  temp dir at once; bind it (`_dir`) for as long as the store is used.
- **libmpv tests**: the video and audio viewer tests skip where libmpv is
  missing; CI installs it and sets `HYDRUS_REQUIRE_MPV=1`, which turns a skip
  into a failure. `headless::init` points every mpv player at a null audio
  output, because mpv probing PipeWire, ALSA and JACK in several tests at
  once crashed the test binary.
- **The reference client runs its own worker threads.** In recorders, pause the
  importers you make and give the client a moment between steps that need it.
- **`git stash`/`pop` touches every file's mtime** and forces a needless UI
  rebuild.
- **Disk**: `target/` grows past 20 GB. `dev.sh` drops
  `target/debug/incremental` when under 5 GB are free.
- **Watch loops**: `pgrep -f <pattern>` (and `pkill -f`) match the calling
  shell's own command line. Wait on an output file, or use `pkill -x`.
