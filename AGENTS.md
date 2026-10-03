# Working on hydrus-rs

This repository holds two things: **hydrus**, the Python client in
`hydrus/` (the *reference*), and **hydrus-rs**, a Rust port of it in
`crates/`. Work here is porting the reference's behaviour to hydrus-rs,
proven against the reference by recorded oracles. These notes are for
coding agents (Codex reads `AGENTS.md`, Claude Code reads `CLAUDE.md`,
which points here) and for the humans directing them.

Read first, in this order:

1. `docs/rust/ARCHITECTURE.md`: the crates and how they fit.
2. `docs/rust/CONVENTIONS.md`: the rules every crate follows.
3. `docs/rust/ROADMAP.md`: what is being worked on, what is next, and
   what is half done.
4. `docs/rust/GUI.md` (what the GUI does) and `docs/rust/DIFFERENCES.md`
   (where hydrus-rs differs from the reference, or lacks something), for
   the area you work on.
5. `oracle/README.md`: running the reference to record ground truth.

## The loop, for each slice of behaviour

Keep slices small: one dialog's list, one button's questions, one page's
box. Each one is a commit that passes CI on its own.

1. **Read the reference.** Find the code (`grep -rn` in `hydrus/`; GUI
   code is in `hydrus/client/gui/`, the model in `hydrus/client/importing/`,
   `hydrus/client/media/` and so on). Note every string the user sees,
   every question asked, and what each button does.
2. **Record it.** Write or extend a recorder in `oracle/` that drives the
   real reference client and writes a fixture to `oracle/fixtures/`.
   Copy the pattern of a recent one: `oracle/record_downloader_lists.py`
   (a page's sidebar, driven step by step), `oracle/record_subscriptions_list.py`
   (a dialog's rows and buttons), `oracle/record_sessions_menu.py` (menu
   entries and their dialogs). They use `hydrus_driver.run_client` on the
   `basic` fixture, run their work on the Qt thread with
   `controller.CallBlockingToQt`, replace the reference's dialogs
   (`ClientGUIDialogsQuick.GetYesNo`, `EnterText`, `SelectFromListButtons`,
   `ClientGUIDialogsMessage.ShowWarning`, ...) with functions that record
   what was asked and answer from a script, and hold the time still
   (`HydrusTime.GetNow = lambda: NOW`). Run one with
   `QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_x.py`.
   Commit the script and its fixture together. The script's docstring
   says what it records; keep it accurate.
   - The reference client runs its own worker threads. Anything that
     happens "later" (a highlight loading, a watcher being checked) can
     land between your Qt calls: pause the importers you make, and give
     the client a moment (`time.sleep`) between steps that need it.
   - The `basic` fixture has no downloaders; make a gallery URL generator
     in the recorder if you need one (see `record_downloader_lists.py`).
3. **Port it**, as hydrus-rs's design wants, not the reference's
   (`CONVENTIONS.md`):
   - GUI logic with no Slint in it goes in `crates/hydrus-gui-model`
     (fast to build and test: `cargo test -p hydrus-gui-model --test model
     <name>`). Most new GUI work should start here.
   - Slint files are `crates/hydrus-gui/ui/*.slint`; they are wired to
     Rust in `crates/hydrus-gui/src/lib.rs` (the main window) or a module
     per window (`checker_options_window.rs`, `session_dialog.rs`, ...).
   - Lists are `ui/list_table.slint`'s `ListTable` with
     `hydrus-gui-model`'s `list_selection` (click, ctrl+click,
     shift+click, as the reference's lists select).
4. **Test it against the recording.** Model tests in
   `crates/hydrus-gui-model/tests/model/<area>.rs`; GUI tests (headless
   Slint, a real store) in `crates/hydrus-gui/tests/gui/<area>.rs`, all one
   test binary: `cargo test -p hydrus-gui --test gui <area>::`. Replay
   the recording step by step where you can (see
   `tests/gui/downloader_lists.rs`, `tests/model/subscriptions_list.rs`).
5. **Check the tests bite** with cargo-mutants on what you changed:
   ```sh
   git add -N <new files>            # so --in-diff sees them
   git diff -- crates/<crate>/src > /tmp/x.diff
   cargo mutants --in-place --in-diff /tmp/x.diff -p <crate> \
       --cargo-test-arg=--test=gui -- <one test name filter>
   ```
   `--in-place` edits the source while it runs: don't edit those crates
   until it finishes, and check `git diff` afterwards. Only one test name
   filter works after `--`. A survivor means a missing assertion, or code
   that does nothing (both happen); fix it, or say in the commit why it
   stays.
6. **Look at it.** The GUI runs headless in tests, but a person should
   see new windows at least once (`cargo run -p hydrus-gui -- <store
   dir>`; with no display, under Xvfb with `xdotool` driving it).
7. **Write it down.** `docs/rust/GUI.md` says what works, in the user's
   terms; `docs/rust/DIFFERENCES.md` says what is missing or different
   from the reference, and why. Update both in the same commit as the
   code.
8. **Run CI's checks** before pushing: `scripts/check.sh` (everything),
   or `scripts/check.sh hydrus-gui-model hydrus-gui` when only those
   changed. CI denies warnings, including clippy's pedantic set.

## Commits

- Small and self-contained, each passing CI. Fix red CI before starting
  anything new.
- Subject in the imperative, saying what the user gets, often "..., as
  the reference does". The body says what changed and which recording
  proves it.
- CI (`.github/workflows/rust.yml`) runs on pushes to `master`,
  `claude/**`, and `codex/**` branches, and on pull requests.

## Several agents at once

- Give each agent its own area (a dialog, a page, a crate) and its own
  branch or worktree. Areas map to files: `ui/<area>.slint`,
  `src/<area>_window.rs`, `hydrus-gui-model/src/<area>.rs`,
  `tests/gui/<area>.rs`, `oracle/record_<area>.py`.
- The shared hot spots are `crates/hydrus-gui/src/lib.rs`,
  `crates/hydrus-gui/ui/main.slint`, `crates/hydrus-gui-model/src/lib.rs`,
  `crates/hydrus-gui/tests/gui/main.rs`, `docs/rust/GUI.md` and
  `docs/rust/DIFFERENCES.md`. Keep edits there small (a module line, a
  callback's wiring, a paragraph) so merges stay easy; put the bulk of a
  feature in its own files.
- Rebase or merge often; run `scripts/check.sh` after merging.
- When worktrees share a Cargo target directory, set `HYDRUS_FIXTURE_DIR`
  to the current worktree's absolute `oracle/fixtures` path. Otherwise a
  cached `hydrus-testkit` can read the checkout it was compiled in.

## Things that have bitten before

- **Slint**: a `LineEdit` the user typed in drops its one-way `text`
  binding; use `<=>`, or replace the model so the element is made anew.
  A `Window`'s `title` can't be set from Rust; give the window its own
  `window-title` property. `StandardTableView` selects one row only; use
  `ListTable`.
- **Key handling**: the main window's shortcuts are a
  `capture-key-pressed` on the outermost `FocusScope`, so they work
  whatever has focus; a widget's own keys go in its own `FocusScope`.
- **Time**: anything that shows "5 minutes ago" takes `now` as an
  argument, so tests and recordings can hold it still.
- **Disk**: `target/` grows past 20 GB. Delete `target/debug/incremental`
  when space runs low; `HYDRUS_CHECK_LOW_DISK=1 scripts/check.sh` deletes
  test executables as it goes.
- **Watch loops**: `pgrep -f <pattern>` inside a shell loop matches the
  loop's own command line and never ends. Wait on an output file instead.
