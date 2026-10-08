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

## When is a leaf done?

When a test tagged `// leaf: <id>` (the comment on the lines just above
`#[test]`) exercises the leaf's behaviour against the reference and passes on
master. The tracker counts tags; CI runs every test on every push, so a tag on
green master is a passing test. `scripts/track.py check` (run in CI) rejects
tags naming unknown IDs.

Leaves signed off under the retired per-leaf ledger (385, state `carried`)
count as done without a tag. Tag their tests when you touch them.

## States

| State | Meaning | What to do |
|---|---|---|
| `missing` | not ported | port it, test it, tag the test |
| `partial` | some of it ported | finish it (notes say what is missing), tag the test |
| `implemented` | written before 2026-10-08 but never verified against the reference | find the test that covers it; check it really asserts the reference's behaviour; tag it. If there is none, write it. If the code is wrong, fix it. |
| `carried` | signed off under the old ledger | done |

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
`out-of-scope` (remote repositories, IPFS, the PTR) is not counted.

## Editing leaves.json

Edit it by hand only to correct data: a wrong workstream, a leaf that should be
split, a note that is now false. Never edit it to mark progress; tags do that.
