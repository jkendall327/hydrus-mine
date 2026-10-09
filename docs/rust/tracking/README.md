# Work tracking

`leaves.json` lists every GUI feature leaf of the reference client (1,274),
each with its workstream, its state when tracking moved here (2026-10-08),
pointers into the reference and native code, and notes from the old audit.
`scripts/track.py` turns it into a to-do list.

```sh
scripts/track.py                  # summary per workstream
scripts/track.py next database    # what to do next in a workstream
scripts/track.py show <leaf-id>   # one leaf in full
```

## Remaining work (GitHub issues, filed 2026-10-08)

Every leaf not done is listed in exactly one issue, with its reference and
native files and what "done" means:

| Issue | Area |
|---|---|
| #86 | Manage tags dialog: suggested tags, cog, follow the viewer, autocomplete |
| #87 | Idle, CPU-busy, shutdown and sleep maintenance timing |
| #88 | Duplicates: search count, scheduling switches, review progress, prefetch |
| #89 | Import options and shared editors |
| #90 | Downloader and subscription exchange |
| #91 | Shell: popups, About box, window geometry |
| #92 | Options window partials and small remainders |
| #93 | Open findings from the 2026-10-08 batch reviews |
| #94 | Platform-limited features (decision needed; drag out is #26) |
| #95 | Proposed out-of-scope moves (decision needed) |
| #96 | Help > debug remainder (low priority) |
| #97 | Recheck every done leaf against recordings, including the 378 carried |

Work an issue on its own branch; one PR per issue (or per workstream for
#97), with one independent review before merge (see `AGENTS.md`). When a new
gap turns up, file an issue for it rather than a list in the repository.

## When is a leaf done?

When a test tagged `// leaf: <id>` (the comment on the lines just above
`#[test]`) exercises the leaf's behaviour against the reference and passes on
master. The tracker counts tags; CI runs every test on every push, so a tag on
green master is a passing test. `scripts/track.py check` (run in CI) rejects
tags naming unknown IDs.

Leaves signed off under the retired per-leaf ledger (378 still untagged,
state `carried`) count as done without a tag, but nobody has checked them
against this standard; #97 does. Tag their tests when you touch them.

## Rechecking done leaves (#97)

A done leaf can carry a `recheck` record: `{"date": "YYYY-MM-DD", "class":
..., "by": "<who or which agent>", "note": "..."}`. `scripts/track.py recheck`
summarises them per workstream; `scripts/track.py recheck <workstream>` lists
the done leaves not yet rechecked; `track.py check` rejects malformed
records. A record says what the tests prove as of its date; phase 2 of #97
fixes the non-sound ones and updates the record.

Classes, decided after the pilot on `options-media` (2026-10-08):

| Class | When |
|---|---|
| `sound` | The test drives the real thing (the window, menu or Options row through the GUI, or the real worker/consumer), its expected values come from a recording of the reference (or the behaviour is only strings or defaults checked against the reference), it covers everything the leaf names, and for a setting the code that reads it is exercised. |
| `source-restated` | As sound, but the expected values are literals restated from the Python source, or computed by the same native function under test, where a recording would be possible. |
| `likely-good` | As `source-restated`, but the leaf is only static text (labels, captions, headings, menu names, tooltips, fixed question wording, fixed defaults or choice lists) and the literals appear verbatim in the reference source. The owner decided (2026-10-08) these need no recording; a bug in one is low priority. Anything computed, conditional or behavioural stays `source-restated`. |
| `weak` | A stand-in is tested instead of the real thing; part of the leaf is not covered; or a setting only round-trips while what reads it is untested or absent. |
| `no-test` | A carried leaf with no test found. |
| `wrong` | The test or the native code visibly contradicts the reference. |

Rules for the cases the pilot found ambiguous:

- **GUI test with typed values, model test with the recording.** Sound only
  if the GUI test drives the real path *and* a separate test replays the
  recording against the same function the window calls, for the same
  behaviour. If the recording is replayed against a different function, or
  for different cases than the leaf names, it is `source-restated`.
- **A decision function standing in for the window or daemon** (a test that
  calls `Slideshow::due`, `Zoom::new` or a `pace()` instead of driving the
  viewer, the window or the loop) is `weak`, whatever it compares against
  (as `AGENTS.md` says for tags).
- **Presence instead of content.** A test that checks only that something is
  painted, shown or changed, where the leaf names what is shown, is `weak`.
- **Expected values from the native function under test**, with no recording
  of that function anywhere, are `source-restated` (the check is circular).
- **Stale notes.** When a leaf's notes are false (often "no editable row
  exists"), say so in the recheck note.

Judge blind: an agent rechecking a leaf must not see another agent's
verdict for it (the pilot's second-stage reviewers, shown the first stage's
verdicts, agreed with all 82).

## States

| State | Meaning | What to do |
|---|---|---|
| `missing` | not ported | port it, test it, tag the test |
| `partial` | some of it ported | finish it (notes say what is missing), tag the test |
| `implemented` | written before 2026-10-08 but never verified against the reference | find the test that covers it; check it really asserts the reference's behaviour; tag it. If there is none, write it. If the code is wrong, fix it. |
| `carried` | signed off under the old ledger | counted as done; to be rechecked (#97) |

The states and notes are from an audit made a few days before 2026-10-08;
later work is not reflected. **Read the code before trusting a state**: a
"missing" leaf may already have a window (for example
`audit-media-menu-database-how-boned-am-i` has `how_boned_window.rs`).

## Workstreams

Workstreams group leaves by area so that agents working on different
workstreams seldom touch the same files.

| Workstream | Covers | Main files |
|---|---|---|
| `options-gui` | Options pages: gui, pages, sessions, style, colours, popups, command palette, shortcuts, tag presentation/editing/sort/suggestions, notes, ratings, file search, sort/collect, duplicates, advanced | `hydrus-gui-model/src/options.rs` (`pages()`), `options_*.rs` |
| `options-media` | Options pages: media playback, media viewer, hovers, audio, thumbnails | same, plus `viewer.rs`, `media_view_options.rs` |
| `options-system` | Options pages: maintenance and processing, speed and memory, system, tray, connection, external programs, files and trash, import/export, downloading, viewing statistics | same, plus the consumers (`maintenance_runtime.rs`, store, net) |
| `database` | Database menu: backup, locations, maintenance, regenerate, check and repair, history, password | `database_*_window.rs`, `hydrus-store` |
| `help-debug` | Help > debug (low priority) | `debug_*.rs` |
| `search-pages` | file search page sidebar, page menu, undo, history | `page.rs`, `active_predicates.rs`, `read_autocomplete`, `predicate_editor` |
| `duplicates` | duplicates page and search context | `duplicates_*.rs`, `hydrus-duplicates` |
| `network` | network menu: downloaders, subscriptions, bandwidth, sessions, logins | `*subscription*`, `downloader_*`, `network_*`, `login_*` |
| `media` | per-file actions: manage tags/notes/ratings/times/URLs, viewer, archive/delete | `manage_*_window.rs`, `viewer.rs`, `archive_delete_window.rs` |
| `tags-services` | tag siblings/parents/migration/sync, services review and management, pending | `tag_*`, `services_*`, `local_services*` |
| `files-io` | import/export folders, manual export, import review, File menu entries | `folders_*`, `export_files_window.rs`, `import_window.rs` |
| `editors` | shared editors: string processor, tag filter, import options, sidecars, formulas | `string_processor*`, `tag_filter*`, `import_options*`, `sidecars*`, `formula*` |
| `shell` | notebook tabs, popups, startup password, window geometry, status bar, Help menu | `lib.rs`, `pages.rs`, `popups.rs`, `notebook*` |

Priorities: `low` (Help > debug) is skipped by `track.py next` unless `--all`;
`out-of-scope` is not counted. Each out-of-scope leaf's first note says why:
remote repositories, IPFS and the PTR (owner's priorities); Database entries
for caches the native store does not keep (ADR-1); and, triaged on
2026-10-08, settings that exist only because of the reference's platform
(Qt toolkit and QtMediaPlayer settings, Python library switches, debug
switches for the Qt-embedded mpv player) or of machinery hydrus-rs does not
have (deferred table deletes, background sibling/parent sync). Undo a triage
by setting the priority back to `normal` and removing that note.

## Editing leaves.json

Edit it by hand only to correct data: a wrong workstream, a leaf that should be
split, a note that is now false. Never edit it to mark progress; tags do that.
