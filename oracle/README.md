# oracle

Tooling that runs the **reference implementation** (the Python hydrus client
in `hydrus/`) to record ground truth for the Rust implementation's tests.

Nothing in the Rust test suite runs Python: the outputs of these scripts are
committed under `oracle/fixtures/` and `oracle/recordings/`. Re-running a
script and committing a changed fixture is a deliberate, reviewable act.

## Setup

```sh
python3 -m venv ~/pyenv
~/pyenv/bin/pip install -r oracle/requirements.txt
# the reference client needs these at runtime, even headless:
sudo apt-get install -y libegl1 libgl1 libxkbcommon0 libfontconfig1 libdbus-1-3 ffmpeg
export QT_QPA_PLATFORM=offscreen
```

## Scripts

| script | output |
|---|---|
| `dump_constants.py` | `fixtures/constants.json`: file types, service types, enum codes |
| `dump_tag_cleaning.py` | `fixtures/tag_cleaning.json`: tag cleaning on awkward inputs |
| `record_predicate_custom_defaults.py` | `fixtures/predicate_custom_defaults.json`: actual Qt star actions across all 40 panel families, 1,600 comparability pairs, explicit-input precedence, immediate reset, owner-close persistence and actual ClientOptions serialization with an interior 3/5 rating for native import |
| `dump_system_predicates.py` | `fixtures/system_predicates.json`: search predicate parsing (system predicates and Client API tag lists) over a large corpus |
| `make_import_media.py` | `fixtures/import_media/`: small deterministic media corpus (committed) |
| `make_repository_fixture.py` | `fixtures/legacy_db/repositories.tar.gz` + manifest: `basic` with a tag and a file repository holding pending content |
| `make_fixture_db.py` | `fixtures/legacy_db/<name>.tar.gz` + `.manifest.json`: a populated reference database |
| `dump_legacy_expectations.py` | `fixtures/legacy_db/<name>.expected.json`: what the reference reads from a fixture database, for `hydrus-legacy`'s tests |
| `build_scenarios.py` | `scenarios/*.json`: Client API conformance scenarios (declarative request lists) |
| `record_api.py` | `recordings/*.json`: the reference client's responses to each scenario |
| `dump_duplicate_merges.py` | `fixtures/duplicate_merges.json`: what duplicate decisions with metadata merges leave in the reference's database |
| `dump_note_merges.py` | `fixtures/note_merges.json`: note merging ("merge cleverly") on duplicate decisions |
| `dump_media.py` | `fixtures/media.json`: a generated media corpus and the reference's view of it |
| `dump_bandwidth.py` | `fixtures/bandwidth.json`: bandwidth trackers, rules and the manager stepped through time |
| `record_network_data.py` | `fixtures/network_data.json`: real Qt bandwidth/context/rule rows, current-job rows and reset questions |
| `record_network_sessions.py` | `fixtures/network_sessions.json`: real Qt session/cookie/header rows, editor validation and clear/delete questions |
| `record_clipboard_urls.py` | `fixtures/clipboard_urls.json`: real Qt clipboard watcher changes, independent switches, recognition and failure behavior on synthetic domains |
| `record_tag_archives.py` | `fixtures/tag_archives.json` and `tag_archive_*.db`: real Qt archive inspectors/confirmations, four hash kinds and scope conversion, pair-count gates and actual Python/native-codec/Python SQLite round trips (`--rust-executable`, standalone `archive_exchange_harness.rs` compiled with cached third-party SQLite only) |
| `record_tag_migration_progress.py` | `fixtures/tag_migration_progress.json`: real MigrationJob, settings panel and Qt popup phases/speed, independent close, pause/cancel/dismiss and strict delayed dismissal |
| `record_tag_migration_pause.py` | `fixtures/tag_migration_pause.json`: actual MigrationJob and Qt PopupMessage pause/resume/cancel with 11 identical entries in batches of three |
| `record_tag_migration_filter_summaries.py` | `fixtures/tag_migration_filter_summaries.json`: 12 actual Qt sibling/parent confirmations with equal, asymmetric and equal-text distinct filter rules |
| `record_tag_migration.py` | `fixtures/tag_migration.json`: real Qt migration controls/questions and reference mapping/pair destination changes |
| `record_downloader_interchange.py` | `fixtures/downloader_interchange.json` and `.png`: reference definition formats, recent version upgrades and Rust JSON/PNG exports loaded by Python |
| `record_subscription_add.py` | `fixtures/subscription_add.json`: real Qt separate gallery chooser and subscription editor acceptance/cancellation chain |
| `dump_metadata_flags.py` | `fixtures/metadata_flags.json`: the XMP, IPTC and software/source flags of the corpus and of `fixtures/metadata/` (images carrying those) |
| `dump_delete_lock.py` | `fixtures/delete_lock.json`: Client API deletes, duplicate deletes and trash emptying with the archived-file delete lock on, and each file's inbox state and domains after each |
| `dump_tag_rendering.py` | `fixtures/tag_rendering.json`: awkward tags as `RenderTag` shows them to the user under several presentation options |
| `dump_file_maintenance.py` | `fixtures/file_maintenance.json`: file maintenance on damaged files (wrong metadata, a wrong or forced type, lost hashes, unreadable, missing or altered content, wrong-size or missing thumbnails, stray copies) with the delete lock on: each file's record, disk state and queued jobs afterwards, what `missing_and_invalid_files` holds, and the URLs sent to be downloaded again |
| `dump_file_handling.py` | `fixtures/file_handling.json`: the corpus's transparency at each `file_has_transparency_strictness`, and its zips' types with comic book detection on and off |
| `dump_domains.py` | `fixtures/domains.json`: domain helpers over a public-suffix corpus |
| `dump_url_classes.py` | `fixtures/url_classes.json`: URL classes making referral URLs and next pages |
| `dump_gugs.py` | `fixtures/gugs.json`: gallery URL generators on random searches |
| `dump_formulas.py` | `fixtures/formulas.json`: parsing formulas on random documents |
| `dump_page_parsers.py` | `fixtures/page_parsers.json`: page parsers on random documents |
| `dump_string_processing.py` | `fixtures/string_processing.json`: string processors on random lists |
| `dump_cookie_jars.py` | `fixtures/cookie_jars.json`: cookie jars as the reference pickles them |
| `dump_import_options.py` | `fixtures/import_options.json`: import option defaults and layering |
| `dump_subscriptions.py` | `fixtures/subscriptions.json`: subscriptions and their queries' history as stored |
| `dump_sidecars.py` | `fixtures/sidecars.json`: sidecar routing between sidecar files |
| `dump_auto_resolution.py` | `fixtures/auto_resolution.json`: duplicates auto-resolution rules as stored, including the owner's own (`fixtures/user_auto_resolution_rules.txt`, as their client stored them) |
| `dump_visual_data.py` | `fixtures/visual_data.json`: the visual-duplicates computations, stage by stage |
| `dump_client_options_defaults.py` | `crates/hydrus-legacy/src/objects/client_options_defaults.json`: a new client's options object |
| `dump_casefold.py` | `crates/hydrus-core/src/casefold_table.rs`: Python's `str.casefold`, as a table |
| `record_info_lines.py` | `fixtures/info_lines.json`: each `basic` file's info lines (`GetPrettyMediaResultInfoLines`) at a fixed "now", all of them and the top hover frame's interesting ones, with a new client's options and with the info line options turned the other way |
| `record_thumbnail_selection.py` | `fixtures/thumbnail_selection.json`: v688's default thumbnail grid, on a page in the running reference, driven through a script of plain, ctrl and shift clicks, select all and none, the thumbnail shortcuts' focus moves and files leaving the page, with the files selected and focused after each step |
| `record_status_bar.py` | `fixtures/status_bar.json`: a page's status bar text (`_GetPrettyStatusForStatusBar`) at a fixed "now", for pages of the `basic` files (all, images, one type, animations, videos, one file, empty with and without a search's override) with several selections and each file alone, and with the single file info option off |
| `record_thumbnail_menu.py` | `fixtures/thumbnail_menu.json`: the thumbnail grid's right-click menu (its own `GetMenu`, as a tree of entries, separators and submenus) on pages of the `basic` files (all of them on two domains, the inbox ones, those only in "my files") with several selections, the trash's included |
| `record_viewer_menu.py` | `fixtures/viewer_menu.json`: the media viewer's right-click menu (its own `ShowMenuFromSignal`, captured rather than shown, as a tree, its volume slider as `{"slider", "value"}` and checkable entries as `{"check", "checked"}`) for several of the `basic` files in a viewer on "my files" and on "all local files" (the trash's included), with what the menu reads from the viewer: its zoom, its canvas fit, whether it is at its largest, whether it is fullscreen, and what plays the file |
| `record_slideshow.py` | `fixtures/slideshow.json`: the period the media viewer's slideshow bends to for a file that plays (its own `_CalculateAnySpecialSlideshowPeriodForCurrentMedia`, on a stand-in viewer) for many durations, periods and sets of the slideshow options, with whether it tells the player to stop at the file's end and whether it stops the slideshow; and the slideshow submenu (`ShowMenuFromSignal`'s, its checkable entries as `{"check", "checked"}`) in a viewer of the `basic` files as its slideshow starts, stops, resumes and shuffles |
| `record_import_status.py` | `fixtures/import_status.json`: how the reference words importers' progress, by its own functions: a file log's status (`FileSeedCacheStatus.GetStatusText`, in full and short, with the short summary's new and deleted counts each way) and its `GetValueRange`, and a search log's (`GenerateGallerySeedLogStatus`), for 401 sets of counts by status; tab names, by the notebook's own `_RefreshPageName` on stand-in pages, importers and notebooks of many names, file counts and import progresses, under each option that shapes them; and a download's line, by the reference's own `NetworkJobControl._Update` on stand-in jobs of many statuses, sizes, speeds and states (its left and right texts, gauge and cancel button) |
| `record_media_sort.py` | `fixtures/media_sort.json`: the `basic` files on "my files" and "all local files" as a page sorts them (`MediaList.Sort`, with the options' fallback sort), by every system sort but random, the options' namespace sorts and two more, and each rating service, both ways; and the options' default, fallback and namespace sorts |
| `record_media_collect.py` | `fixtures/media_collect.json`: the `basic` files on "my files" collected as a page collects them (`MediaList.Collect`) by namespaces and rating services, unmatched files collected or left single, then sorted (`MediaList.Sort`) by every system sort but random, the options' namespace sorts and each rating service, both ways: each item a file or a collection's files in order (collected in the page's order, where the reference's order is arbitrary) |
| `record_media_tests.py` | `fixtures/media_tests.json`: in-memory predicate tests on the `basic` fixture's files |
| `record_rating_svg.py` | `fixtures/rating_svg.json`: rating services' SVG icons (bundled, custom, missing) |
| `record_render.py` | `fixtures/render.json`: `/get_files/render` of each kind of static image, as pixels |
| `record_ugoira_render.py` | `fixtures/ugoira_render.json`: `/get_files/render` of the corpus's ugoiras as APNG and animated WebP, with and without timing notes: headers, and each frame's duration, size and pixels |
| `record_viewer_zoom.py` | `fixtures/viewer_zoom.json`: the media viewer's zooms (`CalculateCanvasZooms`) for files of several types and sizes in several canvases, with a new client's options and with changed zoom levels and per-filetype rules (the changed options object included) |
| `record_url_class_search.py` | `fixtures/url_class_search.json`: searches by URL class, through the Client API and in memory |
| `record_search_undo_locked.py` | `fixtures/search_undo_locked.json`: actual populated lock keeps badge/media during synchronized Undo; hidden namespaces still have raw history menu names (executed 2026-10-04 19:56:17 UTC) |
| `record_system_or_activation.py` | `fixtures/system_or_activation.json`: actual main/basic OR result activation, Shift/normal, seeded drafts, system accept/Cancel, outer Cancel, recents, global history and actual DB query counts (18 cases; executed 2026-10-04 20:22:54 UTC) |
| `record_search_predicate_undo.py` | `fixtures/search_predicate_undo.json`: actual Qt frame-global histories, QAction visible-page toggles, OR, editor cancellation, close/restore, clear confirmations, hidden locked query and empty notebook (20 events; executed 2026-10-04 19:44:59 UTC) |
| `record_incremental_number_boundaries.py` | `fixtures/incremental_number_boundaries.json`: actual IncrementalTaggingPanel initial/clamp boundaries, long ASCII/Unicode leading-zero inputs, Qt signed-integer overflow and Python raw-preview digit-limit failures (executed 2026-10-04 21:00:55 UTC) |
| `record_options_geometry_lifecycle.py` | `fixtures/options_geometry_lifecycle.json`: actual DialogManage + all Options pages on Cancel/X/unchanged Apply/own-frame reset; accepted geometry is saved before the frame table commits |
| `record_filename_simple_paths.py` | `fixtures/filename_simple_paths.json`: actual FilenameTaggingOptions.GetTags with Python posixpath/ntpath backends and real controller filtering; records preserved/dropped `srv` prefixes and mixed Windows separators (path-backend evidence, no Windows Qt execution) |
| `record_similar_files.py` | `fixtures/similar_files.json`: the similar-files search on generated near-duplicates |
| `record_auto_resolution.py` | `fixtures/auto_resolution_run.json` + `legacy_db/auto_resolution.tar.gz`: auto-resolution rules run on generated files |
| `record_downloads.py` | `fixtures/downloads.json`: the downloader against a local fake site |
| `record_import_folder.py` | `fixtures/import_folder_run.json` + `legacy_db/import_folder.tar.gz`: an import folder run |
| `record_export_folder.py` | `fixtures/export_folder_run.json` + `legacy_db/export_folder.tar.gz`: export folder runs |
| `dump_gui_sessions.py` | `fixtures/gui_sessions.json`: GUI sessions, their pages and the downloaders in them, with the reference's reading of each |
| `make_bench_db.py` | a large synthetic library in the reference, for benchmarks |
| `add_bench_duplicates.py` | duplicate groups and potential pairs at the target install's scale, added to a benchmark library |
| `bench_api.py` | timings of a Client API request mix against the reference and hydrus-rs |

## The driver

`hydrus_driver.py` boots the unmodified reference client **in-process** with an
offscreen Qt platform, then runs a hook on a worker thread with the live
controller and the client's own Client API (enabled on a local port with fixed
oracle access keys). When the hook returns, the client shuts down through its
normal SIGTERM path and the driver waits for the database to close. One boot
per process; use `run_in_subprocess` for more.

To keep recordings reproducible the driver turns off the reference's
background similar-files search (it adds potential pairs at unpredictable
moments) and pins the member it picks at random to stand for a duplicate
group whose king is out of view to the lowest file id, as hydrus-rs does.

Fixture databases are populated through the same paths real use takes: the
Client API wherever possible, and controller `Write` commands for the rest
(services, siblings/parents, display application, similar-file search).

## Conformance scenarios

A scenario is a list of HTTP requests against a fixture database. Every
scenario is recorded on a fresh boot of a fresh copy of its fixture: even
"read-only" requests can have side effects in the reference (asking for an
unknown hash's thumbnail assigns it a file id), so sharing boots would make
recordings depend on scenario order. The Rust conformance runner
(`crates/hydrus-api/tests/conformance.rs`) replays the same JSON against an
imported copy of the same fixture and diffs the responses.

Request placeholders: `{MEDIA}` becomes the absolute path of
`fixtures/import_media`. Response normalisation: the recording db dir's
absolute path becomes `{DB_DIR}`, and `{MEDIA}` likewise. Binary bodies are
recorded as sha256 + length.

Where the reference is wrong (a bug users would not rely on), the Rust side
does not copy it: the difference is recorded in `docs/rust/DIFFERENCES.md`.

`record_service_bulk.py` records actual local trash clear/undelete and like,
numerical and inc/dec rating-clear panels on a freshly unpacked basic fixture,
including declined/accepted exact questions, enabled controls and reopened
counts. Reference cached rating values intentionally remain recorded separately
from database-backed service counts because bulk rating writes suppress media
content publication. No remote service is exercised.

`record_service_deleted.py` captures physical-storage review's exact two-stage
record-clear decisions, accepted store updates, local domain counts and import
status of a permanent deletion versus a trash file (executed 2026-10-04 21:43:09
UTC on a freshly unpacked basic fixture). The review panel PNG is recorded too.

`record_rating_preview_one_star.py` records four actual numerical example controls
on an uncommitted duplicate of the imported service with allow-zero disabled.
Centre clicks, one/seven-star changes and checkbox toggles capture both rendered
fractions and the separate saved one-star normalization. Executed 2026-10-04
22:14:39–22:14:41 UTC on a freshly unpacked basic fixture; no preview/conversion
hooks or registered services were changed. Fixture: `rating_preview_one_star.json`.


`record_service_rating_preview.py` opens the actual local like/dislike, numerical
and inc/dec service configuration panels on a copied basic fixture. It records
four independent sample controls, normal/right/middle-click decisions, live
colour/shape/star-count/padding/fraction updates, opening numerical conversion,
empty example persistence values and unchanged original services. The counter's
real edit-value dialog receives scripted accept/cancel decisions. Qt example PNGs
are saved beside `service_rating_preview.json`; no fixture service edits commit.


`record_rating_preview_pointer.py` dispatches real Qt numerical example pointer
presses, held left-button motion, outside movement/presses, release, ordinary
motion and clicks on painted fraction text, for all three opening fraction sides.
It records whole-widget positions/dimensions, fractions and the unchanged second
sample, preserves original fixture services, and saves a populated drag-state PNG.

`record_rating_context_sizes.py` drives the four real RatingsPanel double-spin
boxes, their editingFinished consumers, UpdateOptions, reopened controls and
actual option serialization. Five cases cover defaults, independent fractional
values and both bounds. Real RatingLike/Numerical/IncDec dialog and preview
controls capture pixel sizes, including five-digit counter widths; the actual
Manage Ratings dialog captures all three sizes and held-right/left mouse moves.
An abandoned detached Options draft preserves saved values. Executed successfully
2026-10-04 22:36:53–22:36:55 UTC via the serialized `with-oracle` helper, on a
freshly unpacked basic fixture with clean reference shutdown. The fixture and
inspected PNG are `rating_context_sizes.json` and `rating_context_sizes.png`. No
rating handlers or registered services were replaced; the dialog was cancelled.
