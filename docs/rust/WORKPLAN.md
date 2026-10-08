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
