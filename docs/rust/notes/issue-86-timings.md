# Issue 86 timing log

All times UTC, 2026-10-08. Machine: 4 cores, 15 GB RAM, ~30 GB free disk at start.

| Time | Event |
|---|---|
| 15:09 | Session start. Branch claude/issue-86-manage-tags. `git fetch origin master` done. |
| 15:09 | Started `scripts/dev.sh ui` (first build) and `scripts/setup-oracle.sh` in the background. |
| 15:12 | Oracle environment ready (about 3 min). First recorder ran fine alongside the dependency build. |
| 15:24 | **First UI build killed by the OOM killer (SIGKILL on `rustc hydrus_gui_ui`)** after ~15 min of dependencies. Cause: I ran `scripts/dev.sh slint` while it was compiling, and that builds `tools/slint-check` with its own `cargo` (separate `target/tools`, so no Cargo lock stops it) next to the 13 GB UI-crate rustc. Lost ~15 min of dependency build wall time only partly (dependencies stay cached; the UI crate had just started). Lesson: `dev.sh slint` is not safe during a UI build unless `target/tools/debug/slint-check` already exists. |
| 15:26 | Restarted `scripts/dev.sh ui` (now with the first batch of `.slint` edits: tag-list selection, remove/copy/cog buttons). |
| 15:32 | Second `dev.sh ui` finished: **6 min 34 s** (restarted 15:26; dependencies were already built, so this is the pure UI-crate cost). `.slint` change: manage_tags.slint (tag selection, buttons). |
| 15:36 | First model test compile+run after the cog work: 3.5 min (cold hydrus-gui-model test build). Later model runs: ~50 s. |
| 15:41 | First `dev.sh gui manage_tags` after the UI build: ~2 min (compile + link of the GUI test binary). Reruns after Rust edits: ~40 s. |
| 15:44–15:50 | Third UI rebuild: **5 min 45 s**. `.slint` change: manage_tags.slint (immediate mode, close button, PageUp/Down) and tag_suggestions.slint (recent "clear" button). Writing Rust meanwhile; no Cargo allowed in parallel. |
| 15:58 | Full GUI suite (966 tests) after the viewer slice: **4 min 27 s** run (plus compile). Only the known `emoji_fonts` test failed. Started first strict-Clippy run (`dev.sh lint`, all three changed crates). |
| 16:16–16:22 | Fourth UI rebuild: **5 min 20 s** (manage_tags.slint: empty-input Left/Right/Up/Down). Strict Clippy on the three changed crates: 4.5 min cold, ~1 min incremental. |
| 16:32 | PR #100 opened (about 83 min after session start). Pre-push (lint, slint check, all changed-crate tests; 966 GUI tests ran in 250 s) took 7 min 18 s. |

## Summary (so far; CI time added below once known)

- **Total wall time to PR:** 15:09 → 16:32 = 83 min.
- **UI-crate rebuilds:** 4 starts (15:09 first, killed by the OOM killer at 15:24 after the dependencies; then 15:26, 15:45, 16:17) = **3 completed rebuilds of 6.6, 5.8 and 5.3 min**, plus the first build's dependency half (~15 min, unavoidable once). The first build's dependencies compiled in ~15 min and the UI crate itself never finished.
- **Build-wait time:** about 23 min inside UI rebuilds (I could keep editing Rust, the recorders and docs during them, but could not run any Cargo command at all: not tests, not Clippy, not even `dev.sh slint`, because of the 13 GB peak). Another ~37 min idle on foreground Cargo (model/GUI test compiles and runs, two full 4.5 min GUI-suite runs, Clippy 4.5 min cold). So roughly **60 of 83 min were spent with a build or test running**, and **~35–40 min of it with me idle**.
- **Time lost to anything else:** the OOM at 15:24 (my `dev.sh slint` ran `cargo` for the checker while the UI crate was compiling; ~2 min of my time, but it reset the "UI build in progress" by ~2 min and cost a full restart of the UI crate, ~6 min of its compile); one `sed -i` slip of mine in a recorder (a few minutes). No disk problems (30 GB free at start, ended with the usual `target/` growth).
- **Judgement, if a `.slint` edit rebuilt only the affected window (~1/6 of the crate, ~1 min instead of ~6):** the three completed rebuilds would have cost ~3 min instead of ~18, and, more important than the minutes, the "no Cargo while the UI builds" rule would almost never bite: each rebuild blocked every test, Clippy and slint-check run for 5–7 min, which is what forced me to batch `.slint` edits into four rounds and to write tests blind. I estimate the work would have been **15–20 min (about 20 %) faster**, and the batching discipline (planning the Slint for the whole issue up front) would be unnecessary. The first-build cost (15 min of dependencies, then the first UI crate) is a separate, one-off issue and splitting the crate does not change it.

## CI

- PR opened 16:32; first complete CI run (head `aa49b66`, docs-only change to the log) green at 16:51, **18 min** after it started at 16:33. Two earlier runs were cancelled by my own follow-up pushes; a push of any file (even the timing log) restarts the PR's CI.
- After the review fixes (head `3ddd11f`, pushed 16:53), CI green at **17:11** (18 min). Total, session start to CI green on the final code: 15:09 → 17:11 = 2 h 02 min, of which the review round-trip (review findings read 16:45, fixes pushed 16:53) was 8 min of work.
