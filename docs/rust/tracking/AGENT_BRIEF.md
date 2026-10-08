# Brief for a workstream agent

You are one of several agents porting the hydrus GUI to Rust in parallel. Read
`AGENTS.md` and `docs/rust/tracking/README.md` first. Your workstream is named
in your instructions; `scripts/track.py next <workstream> -n 40` lists its
leaves.

## What to do, per leaf

1. `scripts/track.py show <id>`. Read the reference code it points to.
2. **Check the native code first.** States in `leaves.json` are a few days
   stale: a "missing" leaf may already be implemented. Search
   `crates/hydrus-gui/src`, `crates/hydrus-gui-model/src` and the tests.
3. Then one of:
   - **Already implemented and tested:** read the test. If it asserts the
     reference's behaviour for this leaf (strings, choices, effects), tag it
     (`// leaf: <id>` just above `#[test]`). If it only covers part, extend it
     or write a new test, then tag.
   - **Implemented, untested:** write a test that drives it the way a user
     would (the real window or menu, the real store) and compares with the
     reference, then tag.
   - **Implemented but wrong:** fix it, test it, tag it.
   - **Not implemented and small** (an hour or so): implement it (model logic in
     `hydrus-gui-model`, wiring in `hydrus-gui`), test, tag, and document in
     `docs/rust/GUI.md` / `DIFFERENCES.md`.
   - **Not implemented and large:** skip it; report it.
4. Prefer one test per leaf or a small group of related leaves. Put new GUI
   tests in a new file `crates/hydrus-gui/tests/gui/<area>_<topic>.rs`. In a
   shared checkout, do not add it to `tests/gui/main.rs`: add it to your lane
   (`crates/hydrus-gui/tests/lane_<workstream>.rs`, see AGENTS.md, "Several
   agents at once") and run with `DEV_LANE=<workstream> scripts/dev.sh gui
   <filter>`. Model tests go under `crates/hydrus-gui-model/tests/model/`
   (that binary builds in seconds; keep it compiling).
   Edits to *existing* GUI test files must be tags or small and compile-safe:
   they are in the shared binary.

Tag honestly. A tag says "this test proves this leaf behaves as the reference
does". Do not tag on string-only tests when the leaf is an action; do not tag
a parent feature because one child works.

## The loop

- `DEV_LANE=<workstream> scripts/dev.sh gui <module>::` builds and runs your
  lane's GUI tests matching a filter (seconds after the first build); without
  `DEV_LANE` it runs the shared `gui` binary (existing tests). `scripts/dev.sh model <filter>` for model
  tests. `scripts/dev.sh slint` after any `.slint` edit.
- Avoid `.slint` edits when you can: each one rebuilds the 87 MB generated UI
  crate (~7 min, ~14 GB of memory). If you must, make all of them at once,
  check with `scripts/dev.sh slint`, then build.
- Run `DEV_LANE=<workstream> scripts/dev.sh lint hydrus-gui hydrus-gui-model`
  (or the crates you touched) before reporting.

## Sharing a checkout with other agents

Several agents may work in the same checkout at once, on different
workstreams.

- Edit only files belonging to your workstream, plus new files you create.
  For shared files (`tests/gui/main.rs`, `tests/model/main.rs`, `src/lib.rs`,
  `GUI.md`, `DIFFERENCES.md`), make minimal line-sized edits.
- Keep the tree compiling. Work in small steps. If a build fails in a file you
  did not touch, another agent is mid-edit: wait a minute and retry; do not
  "fix" their file.
- Builds share one `target/` directory; Cargo serializes them ("Blocking
  waiting for file lock"). That is expected. Never run `cargo clean`, never
  set a different `CARGO_TARGET_DIR`, never delete files under `target/`.
- Do not commit, stash, reset or check out anything. The coordinating agent
  commits.

## Report

End with a short report:

- leaves tagged (id -> test name), and leaves implemented or fixed;
- leaves found already done but whose state said otherwise;
- leaves skipped as large, with one line on what they need;
- files you changed or created;
- anything in the workflow that slowed you down or misled you.
