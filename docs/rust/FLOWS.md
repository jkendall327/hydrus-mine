# User flows against the reference

Leaf tests check one behaviour at a time. A **flow** runs several steps a
user would take, in the reference and in hydrus-rs, and compares what the
user can see after each step. It catches what leaf tests cannot: a setting
that never reaches the code that uses it, the order of steps, state carried
from one step to the next, a worker that is never told to start (issue
#166).

A flow has two halves:

- a **recorder**, `oracle/record_flow_<name>.py`, which drives the running
  reference through the steps with its real dialogs and workers and writes
  `oracle/fixtures/flow_<name>.json`;
- a **flow test**, `crates/hydrus-gui/tests/gui/flows/<name>.rs`, which
  drives the headless GUI through the same steps on a real store, with the
  work done as `hydrus serve` does it, and compares each step's end state
  with the recording.

Flows are not leaves: tag a flow test `// leaf:` only if it really proves a
whole leaf.

## The harness

Python, `oracle/flows.py`:

| part | what it does |
|---|---|
| `Site`, `booru_phase` | a fake booru on localhost (posts with tags, a file and a source link; tag searches three posts a page, newest first), served in phases, every request noted (`take_hits`) |
| `definitions`, `install` | URL classes, parsers and a GUG for it, built with the reference's own classes, installed in the client; new traffic unpaused, no wait between a subscription's gallery pages. `install` returns the domain manager for hydrus-rs |
| `scripted_dialogs` | replaces `DialogEdit` and `DialogManage`'s `exec`: each dialog, as it opens, is handed to the next script for its panel's class, which acts on it as a user would and accepts (a `DialogManage` through its OK, which commits its panel) or cancels it. A script that raises cancels its dialog and fails the flow |
| `wait_until_idle` | until the subscriptions manager has nothing running or due within a minute, the network engine no jobs and the database no queued work, all for 1.5 seconds together |
| `popups_shown` | the popups the popup manager holds, not dismissed (a file popup merged into an earlier one of the same label never shows on its own) |
| `subscriptions_state`, `file_log_state`, `files_state` | each subscription and its queries' file and search logs; an importer's file log; each file the logs name, with its tags by local tag service, URLs, inbox and local domains |
| `normaliser` | the site's address becomes `BASE`, dates `TIME`, time deltas ("5 minutes", "just now") `DELTA` |

Rust, `crates/hydrus-gui/tests/gui/flows/mod.rs`, the same parts:

| part | what it does |
|---|---|
| `basic_store`, `install` | the `basic` fixture imported; the recording's domain manager installed as a migration installs it, with the same settings `flows.install` changed |
| `Site` | the recording's site (its port and phases) served by axum, requests noted; `wait_for_a_request` fails if work the reference starts at once does not start promptly |
| `Daemon` | what `hydrus serve` does for the client, in the test process: the download queues, the subscription runner, the client's nudges taken every second, and the queues' live status published four times a second (what the pages' status lines show). `idle` is the reference's idle: no download or subscription running, nothing nudged, nothing due within a minute. A queue's task lives as long as its page, so `idle` looks only at each queue's current file and gallery jobs, and a page's own condition (its seeds all done) says when it has finished |
| `wait_until_idle` | runs the GUI's timers until the daemon is idle and the flow's own condition holds, for 1.5 seconds together |
| `State` | a step's end state, read from the store and the main window (its popups) and shaped as the recorder writes it: `subscriptions`, `file_log`, `files`, `popups`, and `step` for a subscription flow's whole step |
| `compare`, `Known` | every difference between the steps and the recording, by JSON path; each must be listed as `Known` with its `DIFFERENCES.md` entry, and each known one must still differ |

The end state compares what a user sees, not databases: seed statuses are
the reference's codes, files are keyed by SHA-256, the order of the site's
requests within a step is not compared (the reference works pages and
files side by side).

What a flow does not compare:

- **Times.** Dates become `TIME` and relative times ("5 minutes ago",
  "in 3 days", a whole "now") `DELTA`: masked, not compared. Of a query's
  check times only whether it has been checked is compared.
- **Input devices.** Both sides drive the windows' own handlers (the
  reference's panel methods such as `Add` and `_ManageTags`, hydrus-rs's
  Slint callbacks such as `invoke_add`), not mouse clicks or key presses.
  The leaf tests cover those.
- **All of `hydrus serve`.** The in-process `Daemon` runs the download
  queues, the subscription runner and the nudge loop. It leaves out the
  network runtime publisher (`publish_runtime`, the client's network jobs
  panel), import and export folders and the
  Client API listener; a flow needing one adds it.

Each flow's site has its own port (`flows::PORTS`, checked against the
recordings by a test), as the GUI tests run side by side. Both sides refuse
to run when a proxy in the environment would carry requests to 127.0.0.1
(add it to `NO_PROXY`).

## Adding a flow

1. **Pick the steps** and the site they need. Reuse `booru_phase`, or add a
   site kind to `flows.py` (an imageboard thread, a login page) with its
   definitions.
2. **Write the recorder.** Copy `oracle/record_flow_subscription.py`:
   - pick a site port no other recorder or test uses (the URL classes name
     it, so the Rust test serves the same port) and add it to
     `flows::PORTS`;
   - boot the `basic` fixture with `network = True`, `flows.install` first;
   - drive each step through the real GUI (`controller.gui`'s own
     methods, on the Qt thread), scripting every dialog it opens, then
     `flows.wait_until_idle`, with an `also` condition for the step's
     own worker (a dialog's thread, a page's importer);
   - after each step, append `{step, dialog, subscriptions, files, popups,
     hits}`; `dialog` holds what the scripts saw (rows, starting values).
   Run it with `QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python
   oracle/record_flow_<name>.py`, read the fixture, and check it is what
   a user would see. Commit the script and fixture together.
3. **Write the flow test** in `crates/hydrus-gui/tests/gui/flows/<name>.rs`
   (a `mod` line in `flows/mod.rs`): the same steps through the headless
   GUI's windows, `wait_until_idle` after each, `state.step(...)` with the
   same `dialog` keys, then `compare`.
4. **Every difference is a finding.** Fix it when it is small and in
   scope; otherwise add it to `DIFFERENCES.md`, open an issue, and list it
   in the flow's `KNOWN` with that entry's heading.
5. Say what the flow covers in the table below.

Downloader pages refresh from the store on the real client's timer, which
headless tests don't run: call `(bound.sync)()` in the step's condition.
Popups and thumbnail actions need the main window shown (`ui.show()`).

Run one flow with `scripts/dev.sh gui flows::<name>`. A flow takes tens of
seconds: the daemon and the reference pace their work in seconds.

## Flows

| flow | recorder | steps | findings |
|---|---|---|---|
| subscription | `record_flow_subscription.py` | network > subscriptions…: add (downloader chooser, edit dialog, a query added through its dialog), apply; the subscription runs (two search pages, five files); a sixth post; "check queries now", apply; only the new file is fetched; the dialog reopened | hydrus-rs's dialog did not tell the daemon it saved: a new subscription waited up to 30 seconds to start, and "check queries now" just after a run waited two minutes. Fixed: "apply" leaves a nudge that has the daemon take the subscriptions afresh, as `SetSubscriptions` does |
| download | `record_flow_download.py` | a new URL downloader page, a post's URL entered; the file downloaded; its thumbnail clicked, manage tags ("favourite" in my tags), apply; archive; delete, accepting what it asks | the delete question had hydrus-rs's own wording ("Delete this file from my files?"); now the reference's ("Send one file from my files to trash?") where the reference shows one action. Known: the reference asks in a "Delete files?" dialog, hydrus-rs in the main window (#187) |
| tag relationships | `record_flow_tag_relationships.py` | tags > siblings: `doggo` → `dog` on "my tags", added, apply; tags > parents: `dog` → `animal`, added, apply; on a search page of every file one file is tagged `doggo` and another `dog` (the tag itself chosen among the suggestions); searches for `dog`, `animal` and `doggo`, each with all its files selected; the dropdown for `dog`, `doggo` and `ani` (each after the dropdown was cleared, so its rows are that text's). The reference side runs tag display maintenance to the end after each dialog (`sync_tag_display_maintenance`), which the reference does in the background when idle; hydrus-rs has nothing to wait for. Compared: the windows' title, tabs, status line and pair rows; each file's tags as stored and as displayed; the search results; the sidebar tag list's rows with counts; the dropdown's rows with counts | none beyond the known status line (the reference's sync sentences; hydrus-rs applies pairs as it writes). Not compared: the reference's dropdown for one or two characters, which lists `dog` for `d` and `do` although the reference's own database search for those is empty (a stale-results quirk the recorder could not pin down); hydrus-rs lists nothing, as the "do-not-autocomplete threshold" says (see DIFFERENCES.md, "Tags > autocomplete for one or two characters") |
| gallery | `record_flow_gallery.py` | a new gallery downloader page, the downloader chosen, "stop after this many files" set to 5, `blue_eyes` entered (eight posts, three result pages): the search reads two pages and stops at the limit with five files; then `red_hair` entered: one page, three files; then the first query's own limit raised to 20: it has no page left to read (the limit stopped it adding the next), so nothing more is fetched, but "hit file limit" clears. Compared: each query's file log and search log, files (tags, URLs), the list's rows and totals, the highlighted query's box, thumbnails | a search that had hit its file limit showed nothing in the page's search status; the reference shows "hit file limit" (the veto `CheckCanDoGalleryWork` raises). Fixed in `run_gallery`, which now sets it while the limit is hit and clears it once raised. The flow's daemon now publishes the queues' live status, as `serve` does |
| watcher | `record_flow_watcher.py` | a new watcher page, a thread's URL entered; the highlighted watcher checks the thread (a subject, a board tag, three posts with a poster tag each) and fetches its three files; the site adds a post, "check now" on the highlighted watcher; the second check fetches only the new file. Compared: file log, check log, the checker's timing state, the list row, totals and highlighted-watcher texts, thumbnails, files | none: hydrus-rs matched the reference at every step. "just added" in the list row is masked (it shows for fifteen seconds, which a slow machine passes). Files get no tags in "my tags" on either side (the default import options add none), so tags are compared in the file log |
