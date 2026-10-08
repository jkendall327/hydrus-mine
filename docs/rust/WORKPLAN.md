# Workplan: faster verification, then parallel feature work

Started 2026-10-08 by the owner's request. This file is the durable task list
for the agent driving this work; it survives context compactions. Keep it
current: tick items, add findings, record decisions. Newest notes at the bottom.

## Why

The 2026-10-08 review found:

- Agents could not compile the GUI in their 16 GB sandboxes. The generated
  Slint crate (24.5k Slint lines -> 87 MB of Rust) was OOM-killed at ~13.8 GB
  in five of eight configurations. Every GUI change was first compiled by a
  hosted run (median 31 min, p90 59 min, 1.86 runs per pass).
- Once that crate is built, the local loop is fast: a GUI test edit rebuilds in
  15 s, a GUI source edit in 20 s, a model edit in 25 s; one GUI test module
  runs in ~1 s; strict Clippy on the GUI after an edit takes 13-23 s.
  A `.slint` edit costs ~6m40s (the whole generated crate).
- Per-leaf publication (exact-source reruns, census/remap packets, double
  reviews, browser checks) dominated the effort: on 2026-10-07, 621 lines of
  product code against 4.6 million lines of evidence. 135 of the 136 leaves
  signed off since 2026-10-05 were written by 2026-10-05.

## Phase 1: the new workflow (do first)

- [x] 1.1 Cargo profile: `hydrus-gui-ui` built without debug info and without
      incremental compilation (measured: the only way it fits in 16 GB).
- [x] 1.2 `cargo xtask slint-check`: run the Slint compiler alone (~9 s).
- [x] 1.3 `scripts/dev.sh` (or similar): the inner loop. Slint check, build the
      UI crate on its own when Slint changed (avoids the pipelining overlap
      that OOMs), targeted GUI/model tests, Clippy on touched crates.
- [x] 1.4 CI: every push runs Slint check, strict Clippy and all Linux tests
      including the GUI, with Clippy and tests as parallel jobs. Drop the
      manual-dispatch-only full lane and the publication-safety lane.
- [x] 1.5 Retire per-leaf publication: delete checkpoint/audit evidence and the
      publication tooling from the tree (history keeps it). Replace the ledger
      with a test-derived one (see phase 3).
- [x] 1.6 Rewrite AGENTS.md around the new loop. Trim ROADMAP.md.

## Phase 2: exercise it

- [x] 2.1 Implement one leaf end to end with the new loop; record timings and
      friction here.
- [ ] 2.2 Experiment: split the generated UI into several crates. Measure build
      time and peak memory for a cold build and a one-window `.slint` edit.
      Pursue only if the numbers justify the churn.

## Phase 3: work tracking and parallel attack

- [ ] 3.1 Analyze the remaining leaves holistically: group into workstreams by
      area and by the files they touch, so agents can work in parallel without
      colliding.
- [x] 3.2 New tracking: a backlog file per workstream (or one file), leaf IDs
      claimed by tests (`// leaf: <id>` tags), a script that reports done/remaining
      from the test suite.
- [ ] 3.3 Trial parallel Sonnet subagents on disjoint workstreams; measure
      throughput and breakage; refine the instructions.
- [ ] 3.4 Hill-climb: alternate implementation and workflow fixes.

## Notes

(append findings below with dates)

### 2026-10-08 (night 1)

- Phase 1 landed on `claude/pensive-darwin-kvjhpq`: profile override for the UI
  crate; `tools/slint-check` (standalone crate outside the workspace, ~10 s
  warm, reports a bad `.slint` in under 1 s); `scripts/dev.sh`; new CI;
  `hydrus-workspace-hack` via `cargo hakari` (needed so `cargo build -p
  hydrus-gui-ui` resolves the same features as the GUI test graph: 18 packages
  differed, and the solo build was not reused); publication evidence and
  tooling deleted (gui-coverage 314 MB in the tree; still in git history);
  AGENTS.md rewritten; ROADMAP trimmed; tracker in `docs/rust/tracking/`
  (`leaves.json` + `scripts/track.py`).
- Cold `dev.sh ui` 571 s (deps partly rebuilt for new features), peak 13.9 GB.
  Then `dev.sh gui export_files::` reused the UI crate: 181 s cold for the GUI
  crate + test binary, 2.8 GB.
- Trial leaf 2.1 `audit-media-context-missing-stats` (clear selected viewing
  stats): the inventory said "missing" but the action already existed (menu,
  question, store delete) with only its question strings tested. Work was:
  read reference, add a test accessor, write one end-to-end GUI test, tag it.
  Test passed first run; edit-to-result ~2 s once built. Lesson: **many
  "missing"/"partial" states are stale; check code first**. Most of the 458
  "implemented" and a good share of the others may be test-and-tag work.
- `dev.sh lint` now applies rustfmt instead of only checking (CI checks).
- Disk: the session allowance is ~40 GB; `target/` reached 20 GB with stale UI
  artifacts (737 MB rlib each) from experiments. Parallel worktrees need care:
  share one target dir (`CARGO_TARGET_DIR`); Cargo's lock then also serializes
  builds, which keeps memory safe.
- **The generated UI crate is no longer a workspace member**
  (`generated/hydrus-gui-ui`, `exclude = ["generated"]`). As a member it was
  compiled through Clippy's driver, which spent 33+ min of CPU on it; a cold
  `dev.sh lint` went from >35 min to **5.6 min**. Two traps on the way: the
  `"*"` dev override then optimised it at opt-level 2 (OOM; pinned 0), and
  Cargo strips debuginfo when a build's root has none, so the solo build and
  the in-graph build were different units (pinned `strip = false`; verified
  `Fresh`).
- 2.2 split experiment, measured without committing: generating each of the
  121 windows alone sums to 196 MB (shared components duplicate); six area
  groups (MainWindow, Options, four bins of dialogs) total 99.5 MB, largest
  20.5 MB (vs 87 MB in one crate). Expected: `.slint` rebuild ~7 min -> ~1.5-2
  min, peak memory ~14 GB -> ~3.5 GB. Cost: 24 Slint structs are shared across
  groups (`TableRow` in 51 Rust files; 67 files touch a shared type), so each
  group crate would have its own copies and Rust needs conversions or a
  per-group import change. **Deferred** until the parallel trial shows whether
  `.slint` rebuilds actually block agents.

### Parallel trial, round 1 (3 Sonnet agents, shared checkout)

- Workstreams: `network`, `editors`, `tags-services`; plus me on `database`.
- **Hazard seen within minutes:** all GUI tests are one test binary, so one
  agent's half-written test file stops every agent's GUI build (and Clippy).
  Seen twice (missing `ComponentHandle` import for `clone_strong`). Fix for
  round 2: give each agent its own test target (a `[[test]]` harness that
  `#[path]`-includes the common module and the agent's new modules), merged
  into `tests/gui/main.rs` at integration. Edits to existing test files stay
  tag-only or quick.
- Found while doing `database > how boned am I?`: windows whose worker uses
  `slint::invoke_from_event_loop` cannot be tested headless (Slint has one
  process-wide event-loop proxy). Converted that window to a timer-polled
  mailbox (the codebase's usual pattern); rule added to AGENTS.md.
- **Round 1 result:** tags-services 60, network 63, editors 27 newly tagged
  (agents ran 16-23 min each); 385 -> 535 done. No product bugs found; nearly
  all work was finding or writing the test that proves existing code. Agents
  themselves flagged partial tags; I untagged two (commit-pending permission,
  options presentation) and documented one wording difference. Full model
  suite 532/532, GUI 735/736 locally (emoji_fonts depends on system fonts;
  passes in CI).
- Friction reported: the 120 s tool timeout on `dev.sh` while the cargo lock
  is held (use longer timeouts / background); `cargo fmt -p` formats whole
  crate (other agents' files); cross-agent compile breaks (lanes fix).
- Integration recipe that worked: verify "tag-only" files with
  `git diff -U0 | grep -v '^+// leaf:'`, run the agent's new tests, stage only
  that agent's `mod` lines with `git update-index --cacheinfo` (append to the
  HEAD version), commit per agent.
- **Round 2** (4 agents in lanes, 18-44 min each): search-pages 142,
  options-system 49, database 33 (+16 marked not applicable), options-gui 50.
  Done 535 -> ~830 of 1,274. One product change (worker pace extracted into
  testable methods) and test accessors; no bugs found. I untagged 4 tags that
  lacked a real consumer (deferred-delete switches, preview-window rating
  sizes). Full GUI suite 819/820 (emoji_fonts, font-dependent), strict Clippy
  clean.
- Lanes worked: no agent reported being blocked by another's tests, only by
  half-edited `src/` files (unavoidable with one crate). New friction: the lane
  needs `#![allow(dead_code)]` for `common`, or leave `common` out; lanes cost
  ~1.5 GB of disk each while active (delete at integration); `dev.sh lint`'s
  `cargo fmt --all` touches others' files.
- Remaining shape: ~140 `implemented` (options-media, duplicates, files-io,
  media, shell), ~85 partial, ~240 missing (77 debug). The verification sweep
  is nearly exhausted; next is real implementation, where `.slint` edits (and
  so the UI split) start to matter.

### UI split: design (to try after round 3, with the machine to itself)

The verification sweep is nearly done; implementation needs `.slint` edits,
and today each one is a ~7 min, ~14 GB rebuild that blocks every agent on the
shared Cargo lock. Design that avoids most of the Rust churn:

- `generated/hydrus-gui-ui-shared`: compiles an entry exporting every Slint
  `struct`/`enum` that more than one group uses (24 today), so they are
  defined once.
- `generated/hydrus-gui-ui-<group>` (main, options, and ~4 area groups):
  each compiles its group's entry file; its build script then replaces each
  shared struct's generated definition (always the two-line form
  `# [derive (...)] pub struct r#Name { ... }`, no trait impls) with
  `pub use hydrus_gui_ui_shared::r#Name;`, so `TableRow` etc. are one type
  everywhere. Window types are unique to their group.
- `generated/hydrus-gui-ui` stays the facade hydrus-gui depends on:
  `pub use` of every group. Globals (`Theme`, `MenuChoicePolicy`) exist per
  group; the facade re-exports main's explicitly, and the ~15 Rust sites that
  use a global on another group's window name that group's path.
- Expected: largest crate 20.5 MB (from 87); a dialog `.slint` edit rebuilds
  one group (~1.5-2 min, ~3.5 GB) plus hydrus-gui; parallel group builds need
  `-j2` to stay under 16 GB on a cold build.
- Measure: cold build, one-dialog edit, MainWindow edit, peak memory. Keep
  only if the edit loop is at least 3x faster.
- **Round 3** (4 agents, 15-26 min each): duplicates 29 (+2 out of scope),
  files-io 28, media 33, options-media 33. **First real bug found** (files-io):
  declining File > restart left the next plain close restarting; fixed. Also
  two small reference-parity fixes in manual export. CI's round-2 run: all
  Linux tests passed (incl. emoji_fonts); lint failed only on a stale
  workspace-hack (the UI crate left the members, so hakari dropped deps);
  regenerated, and the solo UI build is still reused (0 feature differences).
- Done after round 3: see `scripts/track.py` (about 930 of 1,274).
- Every agent still hits: `dev.sh lint` formatting in lanes (fixed in dev.sh
  after round 2's reports; agents launched before saw the old script), the
  120 s tool timeout, needing `#[path]` re-declarations of helper modules
  (`subscriptions`, `duplicate_filter`) in lanes.
