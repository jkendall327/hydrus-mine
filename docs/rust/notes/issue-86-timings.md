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
