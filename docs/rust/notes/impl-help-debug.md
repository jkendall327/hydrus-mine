# help-debug workstream report

Branch `claude/impl-help-debug`. 77 leaves at the start, 30 tagged and passing
now (47 left). No `.slint` file was edited, so there was no UI rebuild after
the first one.

## Tagged (30)

Already built, untested before; now clicked in the real Help menu by
`crates/hydrus-gui/tests/gui/debug_menu_actions.rs`, with the effect read back:

- profiling > what is this?; gui actions > make a QMessageBox, make some
  popups, make a modal popup / non-cancellable modal popup in five seconds
  (one test, real five-second wait), reset multi-column list settings to
  default, save 'last session' gui session
- data actions > flush log, force database commit, show env, simulate program
  exit signal
- memory actions > clear all rendering caches; network actions > review
  current network jobs
- debug modes > force idle mode (existing test, tag added)

New behaviour (each with a test in the crate that makes the report):

- report modes: idle, shortcut, gui (GUI tests); subprocess, cache (model
  tests); file import (hydrus-import); network and network (silent)
  (hydrus-net, against a local server); subscription (hydrus-download);
  similar files metadata generation (hydrus-media); file (hydrus-store);
  daemon (GUI test running both maintenance daemons); blurhash mode (loader
  unit test comparing pixels with the blurhash recovery image)
- gui actions > autocomplete delay mode (hydrus-store, real 3 s)
- data actions > scan file storage folders (model test of the scan + GUI test
  with the injected picker)
- debug modes > use faulthandler to log crashes (panic hook writing a
  "client crash" log; GUI test panics a thread)

The shared piece is `hydrus_core::debug_flags`: process-wide switches, a
`report(flag, || text)` helper and a sink the GUI installs (popup + console).
The "report modes" submenu offers only the switches something reads.

## Bugs found on the way

- "force database commit" ran its checkpoint inside a write transaction,
  where SQLite ignores it. It now pauses the store, which checkpoints the
  whole write-ahead log. The new test copies the main file alone and finds
  the row.
- A report can be made from inside a database job (file report mode does),
  so the popup is written by a thread of its own; waiting on the writer from
  the writer would deadlock.

## Not applicable (Qt / Python internals)

- thumbnail debug mode: lightens alternate Qt canvas pages
- allow crashy files in mpv (x3): switches the Python mpv wrapper's crash handling
- force a main gui layout now (`adjustSize`), reload current qss stylesheet,
  reload icon cache, isolate existing mpv widgets, make a parentless text ctrl
  dialog, macos anti-flicker test: Qt widgets, QSS, icon cache, mpv widgets, macOS
- refresh pages menu in five seconds: menus here are rebuilt every time they open
- review threads, show scheduled jobs: Python threads and scheduler
- the three pympler "MEGA-LAGGY" memory-use entries: Python object accounting
- run the ui test / client api test / server test / visual duplicates tuning
  suite (x2): drive the Python test harness
- fake petition mode: no petition panel here to fill with fake data
- do self-sigterm (x2), induce a program crash: process-kill tests; the GUI has no
  SIGTERM handler to test, and a deliberate crash can't be asserted

## Remaining (could be done)

- profile mode (client api / db / threads / ui) and query planner mode: need
  a timing hook on every SQLite connection and request path (rusqlite's
  `profile` callback is the likely route, but it's per connection and adds a
  call per statement); not done to avoid touching the store's hot path
- report modes: callto, canvas tile borders, db, file sort, graphics view
  thumbnail update, hover window, media load, mpv, potential duplicates, pubsub,
  shutdown. Media load and file sort need the path / sort key at the report
  site; the others have no matching place yet
- simulate a wake from sleep: `NetEngine::sleep_check_at` exists, but the GUI
  keeps no shared engine
- run fast / slow memory maintenance: caches already maintain on every
  receive, so a test can't isolate the action
- subscription manager snapshot, publish some sub files in five seconds, what is
  this object? (needs a window: paste a serialised object, show its type)

## Workflow friction

- First build on this machine: `scripts/dev.sh ui` 19 min 13 s (dependencies
  plus the 87 MB generated UI crate). First model test build 5 min 18 s after
  that. First strict clippy over the whole workspace ~9 min; the full
  `pre-push` after it was still running when this was written.
- No `.slint` edits, so no UI rebuild after the first.
- Typical loops: `scripts/dev.sh gui debug_menu_actions::` 55-75 s (GUI test
  binary relink); `scripts/dev.sh model main_menu` 25-60 s;
  `cargo test -p <small crate> --test <file>` 5-20 s.
- A background `cargo check` while the UI crate builds just waits on the build
  lock; don't start one.
- `scripts/dev.sh pre-push` compares to `origin/master`, which a fresh clone of
  this branch doesn't have; it said "no changed crates" until `git fetch origin
  master`.
- Strict clippy is stricter than rustc about tests too (`semicolon_if_nothing_returned`,
  `type_complexity`, `needless_pass_by_value`); `dev.sh lint` catches these
  in about a minute once built.
- The debug switches are process-wide, so tests that flip one take a mutex (GUI
  and model files) or live alone in their own integration-test file (other crates).
- `unsafe` is forbidden in tests (`std::env::set_var`), so "show env" is
  checked against the environment as it already is.
