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
5. **Verify meaningful boundaries and integration.** Replay the reference
   recording and cover cancellation, persistence after reopening, invalid
   input and changes reaching the consumers of edited settings. The owner's
   current breadth-work instruction is to omit mutation testing: use the
   reference, behavioral regressions, rendered UI inspection and independent
   review instead. Do not run cargo-mutants unless the owner requests it.
6. **Keep full visual verification deferred.** The owner's current instruction
   (2026-10-05) prioritizes implementation with cheap validation. Use the reference
   and cached Slint source checks as needed; defer native GUI compilation,
   full suites, rendered review and exhaustive audits to the final project pass.
7. **Write it down.** `docs/rust/GUI.md` says what works, in the user's
   terms; `docs/rust/DIFFERENCES.md` says what is missing or different
   from the reference, and why. Update both in the same commit as the
   code.
8. **Use cheap validation.** Formatting, strict backend/model Clippy/type checks,
   simple builds and targeted fast tests are appropriate. Automatic CI has one
   Linux lint/model lane excluding `hydrus-gui`: its generated metadata exceeded
   the cheap budget (over 12 minutes and about 9 GiB in one compiler process).
   Native GUI Clippy/type checking, `scripts/check.sh`, full workspace/native
   suites, platform runs and audits
   are deferred until the owner requests the final validation pass. Preserve
   authored regressions for that pass. Keep implemented and fully validated item
   counts separate. CI denies warnings, including clippy's pedantic set.

## Commits

- Small and self-contained. Fix build/type errors and other bugs that block
  ongoing work; record non-blocking defects for the deferred validation pass.
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
- Integrate slices as they become ready and use cheap backend/model checks.
  Run strict Clippy before expensive code generation; avoid full GUI compilation
  and suites during implementation mode. Keep one current integration CI run,
  cancelling superseded heads, rather than validating every intermediate branch.
- When worktrees share a Cargo target directory, set `HYDRUS_FIXTURE_DIR`
  to the current worktree's absolute `oracle/fixtures` path. Otherwise a
  cached `hydrus-testkit` can read the checkout it was compiled in.
  Give each worktree a distinct `RUSTC_WORKSPACE_WRAPPER` path as well:
  Cargo then separates workspace artifacts while sharing third-party
  dependencies, avoiding reuse of another branch's modified libraries.
  `cargo clippy` replaces that setting with its own driver: give each
  worktree a copied `cargo-clippy` frontend beside a distinct
  `clippy-driver` path, and put that directory first on `PATH`. The
  frontend uses its own executable directory to find the driver; merely
  symlinking the frontend resolves back to the shared toolchain. The
  driver itself can be a symlink to the installed one. Without this,
  a narrower Clippy invocation can reuse another branch's dependencies.

## Things that have bitten before

- **Slint**: a `LineEdit` the user typed in drops its one-way `text`
  binding; use `<=>`, or replace the model so the element is made anew.
  A `Window`'s `title` can't be set from Rust; give the window its own
  `window-title` property. `StandardTableView` selects one row only; use
  `ListTable`.
- **Slint geometry**: a child-to-parent two-way link keeps the parent's
  existing binding or value. An unbound parent output can therefore replace
  measured child geometry with its default zero. Expose measured geometry
  through read-only direct bindings to structurally present items; the source
  compiler alone does not detect this runtime binding failure.
- **Resizable Slint windows**: use `preferred-width`/`preferred-height` for the
  opening size. A literal `width` binding can constant-fold descendant viewport
  measurements even after the native window resizes. Check real narrow/wide
  measurements when a layout consumer depends on the viewport.
- **Winit event filters**: `on_winit_window_event` replaces the prior callback.
  Compose opening observers and existing drop handlers in one owner-local filter;
  query native geometry there because Slint has not yet updated its resize cache.
- **Key handling**: the main window's shortcuts are a
  `capture-key-pressed` on the outermost `FocusScope`, so they work
  whatever has focus; a widget's own keys go in its own `FocusScope`.
- **Time**: anything that shows "5 minutes ago" takes `now` as an
  argument, so tests and recordings can hold it still.
- **Disk**: `target/` grows past 20 GB. Delete `target/debug/incremental`
  when space runs low; `HYDRUS_CHECK_LOW_DISK=1 scripts/check.sh` deletes
  test executables as it goes.
- **Memory**: on a 16 GB machine, use `CARGO_BUILD_JOBS=1` for GUI test
  code generation. Compiling the large GUI library and its test target at
  once can exhaust memory; two workers suffice for the smaller crates.
- **Watch loops**: `pgrep -f <pattern>` inside a shell loop matches the
  loop's own command line and never ends. Wait on an output file instead.
