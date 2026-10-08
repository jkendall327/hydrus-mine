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
