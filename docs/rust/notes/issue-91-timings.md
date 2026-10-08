# Issue 91 (shell: popups, About box, window geometry): timings

All times UTC, 2026-10-08, one agent, a 4-core 16 GB container with the
checkout to itself. Measured with `date -u` as the work went.

## Log

| Time | What |
|---|---|
| 15:11:14 | Session start. `git fetch`, then `scripts/dev.sh ui` started in the background (UI build #0); `scripts/setup-oracle.sh` ran alongside (done 15:12, about 1 min). |
| 15:11-15:24 | Read the issue, the reference and the native code; wrote and ran three recorders (`record_save_geometry.py`, `record_popup_modal.py`, `record_popup_network_job.py`, each run 15 s to 1 min) while UI build #0 ran. |
| 15:24:27 | **UI build #0 done: 12 m 49 s** from a cold `target/` (all dependencies plus the 87 MB generated crate; no `.slint` change of mine in it: `popup_modal.slint` was written after the build script had read the files). |
| 15:25:02-15:28:43 | `scripts/dev.sh slint` (first run builds the checker): **3 m 41 s**, then about 10 s. Waiting. |
| 15:28:53-15:32:33 | First `scripts/dev.sh model ...` after the UI build: **3 m 41 s** (dependencies of the model test binary). Waiting. Later model runs: 27 s. |
| 15:33:26 | **UI rebuild #1 started.** Cause: one new window, `ui/popup_modal.slint`, plus its export line in `main.slint`. No other `.slint` change in the issue. |
| 15:33-15:39 | Worked while it ran: wrote the model and GUI tests, the about and download code, the docs. |
| 15:39:46 | **UI rebuild #1 done: 6 m 20 s.** |
| 15:39:52-15:41:36 | First GUI test build and run (five filters): 1 m 44 s. Each later edit to `hydrus-gui`'s Rust: 20-41 s to rebuild and relink the test binary; **no relink took over 2 minutes.** |
| 15:44:38-15:49:37 | First strict Clippy run (`dev.sh lint`): **4 m 58 s** (then 30-40 s). One lint finding fixed. |
| 15:51:37 | Code committed. |
| 15:51:37-16:06:36 | `scripts/dev.sh pre-push`: **14 m 58 s** (lint, slint check, tests of changed crates; the GUI binary alone 288 s for 967 tests). Result: 966 pass, the known `emoji_fonts::outline_fox_is_selected_without_replacing_platform_text_fallbacks` fails on this container's fonts. |
| PR_OPENED | PR opened. |
| CI_GREEN | CI green. |

## Other time lost

- Nothing to OOM, disk or lock waits. One fumble of mine: a scripted edit to
  `tests/model/about.rs` duplicated a block (my slicing by the wrong index);
  it cost one failed lint run (42 s) and a few minutes to put right.
- `scripts/dev.sh gui a b c` runs the filters one after another, each time
  through `build_ui` and `cargo test`: five filters cost five invocations.

## Rebuilds I wanted and did not do

- `popup_modal.slint`'s layout: I rendered the dialog once (headless render in
  `shell_modal_popups.rs`, `popup-modal.png`) and would have tried the gauge's
  height, the stop button's place (it sits alone on its own row) and the
  dialog's minimum width, each a few minutes of editing and looking. At 6 m 20
  per `.slint` edit, three such tries were 19 minutes, so I left the layout as
  first written.
- No other Slint change was wanted: the About window needed none, the popup
  download control's change (lingering blank for ten seconds) was Rust only,
  and the geometry work is Rust only.

## Summary

SUMMARY_PLACEHOLDER
