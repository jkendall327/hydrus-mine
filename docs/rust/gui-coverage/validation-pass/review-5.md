# Batch 5 read-only validation review

Checkpoint reviewed: `/workspace/hydrus-mine` at requested `3702cdbee`. No builds, native rendering, mutation runs, source edits or Git writes were performed. Evidence below is source/assertion inspection, not successful test execution. “Retain conditional first_pass” means the bounded claim has adequate authored reference/consumer assertions to stay a proposal; centrally pending CI/render review must pass before fully validated accounting. Paths below are repository-relative unless otherwise stated. Abbreviations in prose: gui/gui-model/store/media/import/download/core denote crates/hydrus-gui, crates/hydrus-gui-model, crates/hydrus-store, crates/hydrus-media, crates/hydrus-import, crates/hydrus-download, crates/hydrus-core. Bare test/module names inherit the full directory in that same evidence paragraph. Exact function names identify the assertion when integration has shifted line numbers.

## api-update-toasts

- `audit-options-popup-notifications-popup-window-toaster-make-a-short-lived-popup-on-cookie-header-updates-through-the-client-api`

**Recommendation:** Retain conditional first_pass / one eligible leaf.

**Evidence:** Reference: PopupPanel.py:43,53,76 and ClientLocalServerResourcesManageCookies.py:65,121,256,331,375–385; oracle/record_api_update_toasts.py and fixtures/api_update_toasts.json contain twenty real authenticated resource calls, Options staging and strict 0/4/5/6-second dismissal. Source: hydrus-api/src/routes/network.rs:92,226 publishes inside successful mutation transactions; hydrus-store/src/api_update_toasts.rs:23,34,61,86,100 loads legacy/native policy and formats sorted categories; hydrus-gui/src/popups.rs:223,234 owns retirement. Tests: hydrus-api/tests/api_update_toasts.rs:15 asserts status, job text/count, finished/noncancellable state, exact deadline, same-header backend approval/reason, denied writes and preceding accepted header on late error. GUI tests/api_update_toasts.rs:89,182 assert actual checkbox Cancel/reopen, concurrent width merge, actual API-produced popup admission/expiry and retired incarnation.

**Findings / caveats:** No unsupported category completion found. Qt recording calls actual resource handlers rather than socket transport; native authored router test fills that boundary. Failure coverage includes invalid/unauthorized requests, not database I/O failure. All native/CI/render results remain centrally pending.

## clear-orphan-files

- `audit-media-database-clear-orphan-files`

**Recommendation:** Demote first_pass to partial; countAsCompletedLeaf=false.

**Evidence:** Reference: hydrus/client/files/ClientFilesManager.py:960 ClearOrphans calls WaitIfNeeded for every file, moves during the locked scan, then uses the actual final/cancel lifecycle; ClientDB.py:4112 implements orphan predicates. Source: hydrus-store/src/orphan_files.rs:43 scan,128 clear; GUI src/orphan_files_window.rs:58 starts the real worker/job. Sole Store assertion at orphan_files.rs:190 creates a stray file and Thumbs.db and verifies moving them. Model orphan_files test checks text. No actual ClearOrphans recording or native workflow assertion is cited.

**Findings / caveats:** The test named strays_are_found_and_moved_and_kept_files_stay creates no kept file, so does not prove the central safety predicate. Delete, real-kept-file preservation, shared-storage rejection, naming conflict, scan/clear cancellation, permission/read failures and folder-picker/confirmation cancel are unasserted. Scan checks cancelled once per prefix (orphan_files.rs:62), unlike per-file reference checks; UI cancellation is sampled only when progress updates write the Job. This cannot support the advertised cancellable scan boundary.

## database-granularity

- `audit-media-database-locations-granularity-client`
- `audit-media-database-locations-granularity-offline`

**Recommendation:** Demote both first_pass leaves to partial; countAsCompletedLeaf=false.

**Evidence:** Reference: ClientFilesPhysical.py:20 estimate and :92 physical migration; ClientDBFilesPhysicalStorage.py:319–383 records the returned prefix map and attempts cancellation/error recovery. Native granularise_store in hydrus-store/src/granularity.rs:207 calls regranularise; physical destination uses outcome.prefixes.entry(target).or_insert_with(base) at :137. Native tests :260 and :290 cover one-base roundtrip, empty Store records, incorrect starting depth and cancellation before the first file move. No actual reference recorder or native question/pause/offline test.

**Findings / caveats at 3702cdbee:** P1: successful 3→2 migration can persist a different destination from the mover. regranularise selects first encountered base; granularise_store discards Outcome.prefixes and rebuilds map from storage rows ordered by prefix (:226–247). With f3aa at B and f3ab at A and base A encountered first, mover merges both into A/f3a; prefix-ordered DB map chooses B/f3a. Persist the mover map first, filling only genuinely absent empty prefixes. P1 recovery: reversal regranularise(to,from) at :213–223 redistributes originally split prefixes and cannot recover original paths; final DB write failure (:249) also bypasses rollback. Journal exact source/destination moves, reverse only those on every failure including DB publish, and preserve original records. Existing cancellation test always returns true before a move, so does not establish rollback after committed movement. Offline cancellation leaves moved prefixes without robust recovery and has no recording.

**Root follow-up repair source review (uncommitted diff, after checkpoint):** Journalled exact moves, physical Outcome.prefixes publication, pre-movement old-map read, collision refusal and rollback of final write_and_refresh failure fix the two identified client defects structurally. New split-location success/cancel/triggered-publication-failure assertions are meaningful and authored only. No other fallible pre-publication operation outside recovery found. Existing offline `regranularise` deliberately does not invoke journal rollback. Remaining P2 in `estimate`: byte length 3 with UTF-8 directory `éa` enters `name[1..]` and panics on a non-character boundary (original line 41); the Python estimator safely ignores that name. Use safe substring/byte validation and a Unicode directory regression. Journal rollback retains newly created empty deeper directories, potentially affecting later estimates after aborted 2→3. Cross-device undo copy failure also has no partial-original cleanup/retry assertion. Keep demotion for absent real reference/native workflow coverage even after source repair; CI/render review remains pending.

## debug-delayed-pages

- `audit-options-help-debug-action-make-a-new-page-in-five-seconds`

**Recommendation:** Retain conditional first_pass / one eligible leaf.

**Evidence:** Reference: actual Help/debug QAction and real CallLater/new_page_query in oracle/record_debug_delayed_pages.py and fixtures/debug_delayed_pages.json; the fixture records two five-second launches, hidden/minimized publication and real GUI page creation. Source: hydrus-gui/src/debug_long_popup.rs:26,50,65,96 owns typed immutable locations, deadlines and weak timer; menu_bar.rs captures payload and lib.rs routes delivery through Pages. GUI tests/debug_delayed_pages.rs:79 asserts menu snapshot, overlap, exact deterministic boundaries, current notebook/tag default and real Store query/thumbnails; :220 checks coexistence with long/delayed popup production; :248 tests cancelled/accepted exit, rebind/drop/lost window; :323 checks chooser destination/anchor preservation.

**Findings / caveats:** Bounded action is supported by explicit consumer assertions. Native deterministic clock tests do not demonstrate physical native five-second timing or backend minimized behavior; those remain pending. New invocation while hidden is intentionally refused. Refresh-pages-menu debug action remains absent and uncredited.

## downloader-update-times

- `audit-options-speed-and-memory-download-pages-update-experimental-minimum-gallery-importer-update-time`
- `audit-options-speed-and-memory-download-pages-update-experimental-gallery-importer-magic-update-time-denominator`
- `audit-options-speed-and-memory-download-pages-update-experimental-minimum-watcher-importer-update-time`
- `audit-options-speed-and-memory-download-pages-update-experimental-watcher-importer-magic-update-time-denominator`

**Recommendation:** Retain four conditional first_pass control leaves.

**Evidence:** Reference: SpeedAndMemoryPanel and ClientGUISidebarImporters._UpdateImportStatus via actual oracle/record_downloader_update_times.py:50–90 / fixtures/downloader_update_times.json; private actual lists and paused managers are driven under a held clock. Source: gui-model/src/downloader_update_times.rs:14–31 implements strict now>deadline, displayed-row sampling, max(minimum,rows/ratio) and zero fallback; gui/src/downloader_update_times.rs and page.rs connect it to the visible current gallery/watcher status consumer. GUI tests/downloader_update_times.rs:72 replays constructor bounds, normalization, Cancel/retained Apply and reopen; scheduler at :133 and tests :586/:590 compare real displayed lists, pending deadlines, forced refresh, busy source, tab roundtrip and retired/hidden/accepted-close behavior.

**Findings / caveats:** The four claimed preferences reach real consumer lists rather than merely saved settings. Recording controls the wall clock/list subsets, not real downloader work. Native tests distinguish status snapshots from always-live details. No new untested failure invariant is included in the proposed finite scope; hosted native executions/render checks pending.

## existing-tags-filter

- `import-existing-tags-filter`

**Recommendation:** Retain conditional first_pass / one implementation action; no parent/shared-filter credit.

**Evidence:** Reference: ClientGUIImportOptionsPanels.py:1103,1167 action and actual GetTags database consumer recorded by oracle/record_existing_tags_filter.py:17 / fixtures/existing_tags_filter.json; real modal child is answered by Qt timers. Source: gui-model/src/import_options_editor.rs TagFilterTarget/set_tag_filter, gui/src/import_options_window.rs on_edit_existing_tags_filter, download/src/content.rs:130–178 service_tags applies selected-service current-mapping checks only to admitted parsed/additional tags. Model tests/existing_tags_filter.rs:9 assert captured-service acceptance, serialization and default retirement; GUI test :7 covers child Cancel, parent staging/persist/reopen and late callbacks (:138–172); download/tests/local_import.rs:81 exercises saved options through actual file import.

**Findings / caveats:** Filter polarity is additionally backed by subscription_import_options.json, not invented native expectations. Native child topology differs from the cog/modal reference and is declared. Wider tag-filter/default UI remains independently scoped. Native/consumer execution pending; no demotion required from read-only evidence.

## file-maintenance-new-work

- `audit-media-database-maintenance-search`
- `audit-media-database-maintenance-schedule`

**Recommendation:** Demote both first_pass leaves to partial; countAsCompletedLeaf=false.

**Evidence:** Reference: ClientGUIScrolledPanelsReview.py:740,831,894,915 contains actual work-selection, 1000/1001 confirmation, schedule and Jobs added paths. Source: gui-model/src/file_maintenance_new.rs:75 resolves selected/default local domain and real search_without_implicit_limit, :114 schedules Store add_jobs. GUI src/file_maintenance_new.rs:67–214 wires asynchronous searches, inline confirmation and workers. Model tests/file_maintenance_new.rs:10 hard-code labels/1000 threshold; :29 performs broad empty-predicate/all-media happy path and adds Blurhash. No executed reference recorder or native test accompanies these two new claims.

**Findings / caveats:** P2 retained owner defect: on_add_job (:177), on_new_answered (:196) and add worker closure (:93) have no current/active/visibility check. file_maintenance_current.rs:471–495 clears slot/question and hides owner on close, while current-tab controls validate visibility at :352; retained old new-tab handle/confirmation can still schedule Store jobs after close. Native close/decline, invalid search, failed search/add and refresh-to-scheduled-list consumers are not asserted. A disabled button alone is not the ownership guard.

## follow-migration

- `audit-media-migration-pause`
- `migration-archive-source`
- `migration-archive-destination`
- `migration-hash`
- `migration-pair-count`

**Recommendation:** Retain five conditional first_pass bounded leaves.

**Evidence:** Reference: executed tag_migration_pause.json, tag_migration_progress.json and tag_archives.json / actual Python archive binaries. Source: store/src/tag_migration/job.rs:355 run_with_events validates endpoints and canonical source/destination identity, owns stable reader, commits batches and rereads count gates; archive.rs:105/129/178 inspect/source/destination/write preserves metadata and atomic codec writes. GUI src/tag_migration_window/progress.rs:42 owns independent progress, pause/cancel, weak lifetime, immediate dismissal when cancelled and strict delayed removal. Tests: store/tests/tag_migration.rs:403 pause after commit/resume/cancel; :607 conversion/scope/filter/archive reopen; :705 export merge/cancelled prefix; :830 counts; :947 wrong-type/hash/missing input; :1008 live gate changes; :1072 exact phase/batch events. GUI tests/tag_migration.rs:165,301,415,473,509 cover settings close, archive choices, cancellation and popup lifecycle.

**Findings / caveats:** Five source/action/hash/count/progress slices have actual reference-backed assertions, including failure/cancel/real destination consumers. Existing-source read-only inference and destination hash pinning are deliberate differences, and native batches preserve committed prefixes rather than Qt long transaction cleanup. The stated standalone codec exercise is author evidence only, not a rerun here. Render/hosted checks pending; zero parent credit.

## hash-predicate

- `audit-options-predicate-hash-hash-hashes`
- `audit-options-predicate-hash-hash-clean`

**Recommendation:** Retain two conditional first_pass leaves.

**Evidence:** Reference: ClientGUIPredicatesSingle.py:1165 actual hash panel and ClientParsing parser recorded by record_hash_predicate.py:18 / fixtures/hash_predicate.json. Source: gui-model/src/predicate_editors/special.rs:429 parser, :593 cleanup, :772 predicate conversion preserves types/sign/empty values and owned warning flow; GUI predicate_editor_window.rs / predicate_notice.rs wire real multiline controls and acknowledgements. Model tests/hash_predicate.rs:75 replay cleanup and explicit predicates. GUI tests/hash_predicate.rs:48 assert resulting real search predicates/default save/reopen; :135 verifies typed text replacement, mixed/bad-line warnings and acknowledgement; :301 onward covers hidden/rebound/accepted-close retirement.

**Findings / caveats:** Evidence goes beyond button existence to native query/default consumers. Actual rendered warning/hash PNG comparison pending; no broad system-predicate parent promotion. Invalid mixtures/removal confirmation and Cancel are explicitly asserted; database I/O failure is not part of the claimed cleanup semantics.

## image-colour

- `audit-options-media-playback-system-apply-image-icc-profile-colour-adjustments`

**Recommendation:** Retain conditional first_pass / one ICC control.

**Evidence:** Reference: record_image_decoder_policies.py and fixtures/image_decoder_policies.json record Qt ICC flag plus real PNG/JPEG/WebP pixels, fallback behavior and Options staging. Source: media/src/imaging/pil.rs:377–418 applies snapshot embedded ICC policy while retaining gamma/chromaticity fallback; store/src/image_colour.rs reads retained legacy/native override; gui/src/image_colour_watch.rs observes owned changes and preview/viewer/filter/import consumers use it. Media tests/image_colour.rs:12 replay pixel arrays, :50 checks future animation policy without resetting position; import/tests/image_colour.rs:10 asserts actual import/thumbnail pixels under live saved changes; GUI tests/image_colour.rs:138,348,497 compare actual canvas pixels, held stale replies/ownership and paused animation frame preservation.

**Findings / caveats:** One setting has substantial pixel/consumer/cancel/reopen evidence. Captured fallback remains active even with embedded ICC off, matching stated reference. Held-results tests are authored, not execution evidence; hosted media/native tests and render inspection remain required. No completion of wider decoder selector/truncation/PIL/system-FFmpeg controls.

## login-step-cookie-list

- `login-step`

**Recommendation:** Retain support-only packet; completion credit=0; no independent first_pass leaf.

**Evidence:** Reference: ClientGUILogin.py:2111 and ClientGUIStringControls.py:302; record_login_step_cookies.py / fixtures/login_step_cookies.json preserve actual embedded topology, sequential name/value matcher modal answers and serialization. Source: gui-model/src/login_workflows.rs StepEditor embeds CookiesEditor; gui/src/login_step_window.rs direct matcher routing; ui/login_step.slint:76 has key/matching table. GUI tests/login_workflows.rs:1237 embedded_step_cookie_actions_replay_qt_and_retire_matchers_with_the_owner asserts fixture rows, Cancel at both stages, selection/delete and retired descendant behavior. Adjacent shared_cookie_list test (:996) and real HTTP login regressions consume saved step matcher fields.

**Findings / caveats:** Manifest claims=[] is intentional; supporting_scope targets login-step parent, which has a content child and cannot count as completed leaf. Shared script-list topology and matcher/parser limitations remain outside this support change. Tests/PNG pending centrally; retain parent improvement only at final integration review.

## media-background

- `audit-options-media-viewer-hovers-background-draw-tags-left-in-the-viewer-background`
- `audit-options-media-viewer-hovers-background-draw-file-information-top-in-the-viewer-background`
- `audit-options-media-viewer-hovers-background-draw-ratings-and-locations-top-right-in-the-viewer-background`
- `audit-options-media-viewer-hovers-background-draw-notes-right-in-the-viewer-background`

**Recommendation:** Retain four conditional first_pass switch leaves.

**Evidence:** Reference: actual MediaViewerHoversPanel + Canvas _DrawBackgroundDetails/_DrawTags/_DrawTopMiddle/_DrawTopRight/_DrawNotes in record_viewer_background_options.py / viewer_background_options.json has 32 switch/hover cases, ordered calls, notes offsets and opaque coverage. Source: gui/src/viewer_presentation.rs:19–22 copies four independent saved preferences to viewer; ui/viewer.slint paints passive metadata before opaque media. Model options_dialog.rs:1687 verifies staged independent controls; GUI options_window.rs:3423 asserts saved fields, native drawing enablement, real metadata/tag/rating/note presence, notes offset, nonzero rendered occupancy and exact opaque-cover occupancy, plus Cancel/reopen afterward.

**Findings / caveats:** Consumer assertion is meaningful but intentionally does not compare exact platform glyphs or every individual text call/pixel to Qt; renderer checks still required. Background and popup/focus settings remain independent. Failure to read settings falls back by design and has no injected I/O assertion. Wider hover subsystem remains Partial.

## media-pointer

- `audit-options-media-viewer-mouse-behaviour-do-not-allow-mouse-media-drag-panning-when-the-media-has-duration`
- `audit-options-media-viewer-mouse-behaviour-hide-mouse-cursor-during-media-viewer-drags`

**Recommendation:** Retain two conditional first_pass switch leaves.

**Evidence:** Reference: real JPEG/GIF Canvas drag methods under controlled unanchored coordinates in record_viewer_pointer_options.py / viewer_pointer_options.json; options staging/serialization captured. Source: viewer_presentation.rs:30–48 loads live preferences and actual duration metadata; viewer.slint:990–1030 captures acceptance on press, hides only after accepted movement, then clears on release. Model options_dialog.rs:1483 stages controls. GUI options_window.rs:2580 dispatches actual Slint press/move/release and compares fixture acceptance, delta and every cursor phase, preserves keyboard pan, verifies Cancel and reopened actual duration media.

**Findings / caveats:** Finite disallow-duration/hide-during-drag behaviors have real consumer assertions. Recording directly calls business pointer methods with coordinates; native test dispatches events. OS cursor warp/anchored behavior/platform loss of capture are separate scope; no parent credit or full visual validation.

## namespace-colour-edit

- `audit-options-tag-presentation-namespace-colours-edit`
- `audit-options-tag-presentation-namespace-colours-namespace-colours-editor`

**Recommendation:** Demote both first_pass claims to partial; countAsCompletedLeaf=false pending edit-specific reference/native boundary evidence.

**Evidence:** Reference: TagPresentationPanel.py:290 obtains selected rows, runs actual QColorDialog sequentially and keeps rejected colour. Existing namespace_colour_controls.json backs prior add/delete/control behavior; this packet cites no executed recording of newly added sequential edit. Source: gui-model/src/namespace_colours.rs:117/126 selects/recolours draft; gui/src/options_namespace_colours.rs:66–131 owns queued picker accept/Cancel with valid()/answered guards. Sole newly cited test namespace_colours.rs:194 directly recolours two selected rows and checks untouched default and selection.

**Findings / caveats:** That test never opens/answers a picker, cancels one row while accepting another, tests parent Cancel/reopen or consumes applied colour. Existing unrelated add/delete oracle does not prove the new sequential edit workflow. Editor dialog ID and edit action ID currently receive duplicate two-leaf credit for the same edit behavior; any editor retention must be grounded in its full existing editor packet/scope, not solely this one direct model method assertion.

## options-reference-rows

- `audit-options-media-playback-mpv-debug-loop-playlist-instead-of-loop-file-in-mpv`
- `audit-options-media-playback-mpv-preferred-audio-output-device`
- `audit-options-audio-the-preview-window-has-its-own-volume`
- `audit-options-connection-general-debug-set-the-requests-ca-bundle-env-to-certifi-cacert-pem-on-program-start`
- `audit-options-exporting-drag-and-drop-bugfix-set-drag-and-drops-to-have-a-move-flag`
- `audit-options-exporting-drag-and-drop-copy-files-to-temp-folder-for-drag-and-drop-works-for-50-200mb-file-dnds-fixes-discord`
- `audit-options-exporting-drag-and-drop-drag-and-drop-export-filename-pattern`
- `audit-options-gui-frame-locations-debug-when-rescuing-resizing-to-media-media-viewer-add-top-left-safety-padding`
- `audit-options-gui-misc-anti-crash-bugfix-use-qt-file-directory-selection-dialogs-rather-than-os-native`
- `audit-options-gui-misc-bugfix-if-on-macos-show-dialog-menus-in-a-debug-menu`
- `audit-options-gui-misc-bugfix-set-child-windows-as-non-tool-flagged`
- `audit-options-gui-misc-test-use-your-locale-for-integer-rendering`
- `audit-options-gui-misc-use-native-menubar-if-available`
- `audit-options-gui-pages-opening-and-closing-bugfix-force-hide-page-signal-when-creating-a-new-page`
- `audit-options-importing-drag-and-drop-when-dnding-a-url-onto-the-program-switch-to-the-page-where-it-lands`
- `audit-options-media-playback-qtmediaplayer-debug-set-null-audio-device-on-silent-media`
- `audit-options-media-playback-qtmediaplayer-debug-use-the-same-qtmediaplayer-through-media-transitions`
- `audit-options-media-playback-qtmediaplayer-test-use-opengl-window-in-qtmediaplayer`
- `audit-options-media-playback-mpv-debug-set-null-audio-device-on-silent-media`
- `audit-options-media-playback-mpv-debug-use-legacy-mpv-communication-method`
- `audit-options-media-playback-mpv-linux-debug-do-not-allow-combined-setgeometry-on-mpv-window`
- `audit-options-media-playback-mpv-test-destroy-recreate-mpv-players-instead-of-recycling-them`
- `audit-options-media-playback-mpv-test-use-the-same-mpv-player-through-media-transitions`
- `audit-options-media-playback-system-allow-loading-of-truncated-images`
- `audit-options-media-playback-system-load-images-with-pil`
- `audit-options-media-playback-system-prefer-system-ffmpeg`
- `audit-options-media-viewer-hovers-hover-windows-pin-the-duplicates-right-duplicates-filter-hover-window-so-it-is-always-visible`
- `audit-options-media-viewer-hovers-preview-window-hovers-draw-ratings-and-locations-top-right-in-preview-window-background`
- `audit-options-media-viewer-hovers-preview-window-hovers-pop-in-this-hover-on-mouseover`
- `audit-options-popup-notifications-popup-window-toaster-freeze-the-popup-toaster-when-mouse-is-on-another-display`
- `audit-options-speed-and-memory-image-tile-cache-ideal-tile-width-height-px`
- `audit-options-speed-and-memory-image-tile-cache-image-tile-cache-timeout`
- `audit-options-speed-and-memory-image-tile-cache-memory-reserved-for-image-tile-cache`
- `audit-options-speed-and-memory-video-buffer-memory-for-video-buffer`
- `audit-options-system-system-sleep-include-the-file-system-in-this-wait`
- `audit-options-system-tray-always-show-the-hydrus-system-tray-icon`
- `audit-options-system-tray-bugfix-do-minimise-hide-using-event-deferred-state-prep-tech`
- `audit-options-system-tray-bugfix-do-minimise-hide-with-post-show-state-restoration`
- `audit-options-system-tray-close-the-main-window-to-system-tray`
- `audit-options-system-tray-minimise-the-main-window-to-system-tray`
- `audit-options-system-tray-start-the-client-minimised-to-system-tray`
- `audit-options-tag-editing-tag-dialogs-number-of-recent-petition-reasons-to-remember-in-dialogs`

**Recommendation:** Demote the two mpv first_pass leaves to partial/count=false; retain all other 40 proposed Partial support rows with count=false.

**Evidence:** Reference: options_dialog.json records exact rows/defaults/ranges; actual mpv consumer ClientGUIMPV.py:348/521 loads preferred device and :1699 configures loop policy. Source: store/src/reference_options.rs owns typed imported preserved values; GUI playback.rs:97–117 reads them at file load, mpv.rs:353–366 emits loop/loop-playlist/audio-device commands. Cited Store test reference_options.rs:207 checks default/load_images_with_pil/qt_style_name values only; model options_dialog::the_options_pages_are_the_references compares registry labels/structure rather than real player commands.

**Findings / caveats:** No asserted mpv command stream, playback change, rejected device/failure, player reuse/reopen or consumer-focused reference recording supports first_pass for the two proposed live fields. Source wiring is present; keep honest Partial pending executor assertion. All remaining forty rows expressly preserve/edit settings without consumers, so their Partial and zero credit are correct; generic label evidence is sufficient only for that narrower support assessment.

## popup-width

- `audit-options-popup-notifications-popup-window-toaster-approximate-max-width-of-popup-messages-in-characters`
- `audit-options-popup-notifications-popup-window-toaster-bugfix-force-this-width-as-the-fixed-width-for-all-popup-messages`

**Recommendation:** Retain two conditional first_pass control leaves.

**Evidence:** Reference: real PopupPanel UpdateOptions and PopupMessage/Manager oldest-ten cohort in record_popup_width.py / fixtures/popup_width.json; old cards retain policy after Apply and newly admitted eleventh uses new fixed policy. Source: store/src/popup_width.rs typed policy; gui-model/src/options_popup_width.rs stages raw/display fields; gui/src/popups.rs caches per-job display policy; ui/popups.slint measures wrapped body at resolved width. Model tests/popup_width.rs and GUI tests/popup_width.rs replay Apply/Cancel/reopen, merge, capped/fixed/variable widths, gauge minimum and admission policy; import decoder test preserves raw options.

**Findings / caveats:** Retain after integrated generic Options visibility guard. Popup-question layout repair is separate supporting evidence, not extra leaf credit. Platform font widths are approximate character units, explicitly distinct from Qt geometry; standalone toaster placement/freeze/monitors remain outside slice. Native execution and authored PNG/pointer layout checks remain pending.

## rating-context-sizes

- `audit-options-ratings-preview-window-preview-window-like-dislike-and-numerical-rating-icon-size`
- `audit-options-ratings-preview-window-preview-window-inc-dec-rating-icon-height`
- `audit-options-ratings-dialogs-dialogs-like-dislike-and-numerical-rating-icon-size`
- `audit-options-ratings-dialogs-dialogs-inc-dec-rating-height`

**Recommendation:** Retain two dialog conditional first_pass controls; retain two preview Partial controls with zero credit.

**Evidence:** Reference: real RatingsPanel double-spin editingFinished/serialization/reopen plus actual Manage Ratings/example dimensions in record_rating_context_sizes.py / fixtures/rating_context_sizes.json. Source: gui-model/src/rating_sizes.rs normalizes recorded two-decimal binary-half boundaries; manage_ratings_window.rs/services_editor_window.rs supply independent dialog/preview size consumers. Model tests/rating_sizes.rs:16 asserts six cases including 31.755→31.75 and 6.125/12.125→.13. GUI tests/rating_sizes.rs:50 replays real staged Options Cancel/reopen and actual Manage Ratings/service-example sizes, then dispatches held-button pointer interactions and asserts persisted file ratings.

**Findings / caveats:** Absent preview canvas makes preview rows Partial even though examples consume them; manifest correctly avoids counting preview completion. Dialog behavior has exact consumer assertions. Geometry/pointer PNG inspection remains centrally pending; broader ratings services/icons unaffected.

## search-predicate-undo

- `audit-options-search-undo`
- `audit-options-search-undo-undo-predicate-additions`
- `audit-options-search-undo-undo-predicate-removals`
- `audit-options-search-undo-clear-search-history`

**Recommendation:** Retain all four existing Partial assessments/count=false.

**Evidence:** Reference: record_search_predicate_undo.py / fixtures/search_predicate_undo.json records real frame-global histories/actions/clear; search_undo_locked.json establishes actual populated locked media retention; system_or_activation.json covers OR/edit Cancel. Source: gui-model/src/predicate_history.rs:25,36,52 maintains distinct identity-based recency buckets; menu_bar.rs and page/pages.rs apply selected entries to current query. Model tests/predicate_history.rs:36/153 replay deltas/recency/clear and stale/refused inputs. GUI predicate_history.rs:48/185 asserts visible, cross-page/close-restore, locked, empty-notebook and hidden-namespace behavior.

**Findings / caveats:** The authored regressions match substantial behavior, but packet explicitly preserves Partial for missing reference status-bar explanations and deterministic within-batch ordering. No first_pass/count promotion is warranted from this support change; native execution pending.

## shortcut-content-commands

- `audit-options-nested-shortcuts-command`

**Recommendation:** Retain Partial/count=false support claim.

**Evidence:** Source: core/src/shortcuts.rs content command data, gui-model/src/shortcut_sets.rs command choices/descriptions, gui/src/shortcut_windows.rs draft editor and shortcut_runtime.rs media-viewer content executor; ui/shortcuts.slint uses typed tag/rating fields. Cited model tests/shortcut_sets.rs:22,44,84 prove names/descriptions/default sets/custom set edits; they do not prove actual tag/rating writes.

**Findings / caveats:** Full command catalogue, repository tags, rich autocomplete/rating controls and thumbnail execution are explicitly absent, so Partial is correct. Actual consumer command dispatch, invalid typed values, owner Cancel/reopen and custom viewer default persistence need stronger native assertions before any future leaf completion. Do not treat generic shortcut recording as proof of all content commands.

## sidebar-sort-collect-cogs

- `audit-options-nested-sort-collect-cog-default-collect`
- `audit-options-search-sort-collect-tag-service`
- `audit-options-search-sort-collect-tag-display`

**Recommendation:** Retain three conditional first_pass finite controls.

**Evidence:** Reference: ClientGUIMediaResultsPanelSortCollect.py menus/default collect and actual group/sort consumers via record_sidebar_sort_collect_cogs.py / sidebar_sort_collect_cogs.json. Source: gui-model/src/sort_cog.rs:6–125 generates contextual groups and modifies only selected service/display type; gui/src/sidebar_context_cog.rs:125 owns captured page/options targets, unique menu IDs and visible guards. GUI tests/sidebar_context_cogs.rs:142 replay exact service/display/unmatched menus and real resulting media groups/order (:232–277), hidden/stale/successor actions (:283–317); :321 stages default collect and verifies Apply/Cancel/reopen/retired actions.

**Findings / caveats:** Consumer assertions distinguish storage/display, independent sort/collect contexts and locked media; adequate bounded evidence. Remote service implementation is not implied by menu service listing. Exact native menu PNG/geometry review and execution pending; no parent credit.

## tab-drag

- `audit-options-gui-pages-navigation-and-drag-and-drop-selection-chases-dropped-page-after-drag-and-drop`
- `audit-options-gui-pages-navigation-and-drag-and-drop-with-shift-held-down`
- `audit-options-gui-pages-navigation-and-drag-and-drop-navigate-tabs-during-drag-and-drop`
- `audit-options-gui-pages-navigation-and-drag-and-drop-with-shift-held-down-2`
- `audit-options-gui-pages-navigation-and-drag-and-drop-bugfix-disable-all-page-tab-drag-and-drop`
- `audit-options-gui-pages-navigation-and-drag-and-drop-experimental-mouse-wheel-scrolls-tab-bar-not-page-selection`

**Recommendation:** Retain four conditional first_pass preference leaves; retain two navigation Partial leaves/count=false.

**Evidence:** Reference: actual option and notebook pointer/drop/scroll decisions in record_tab_drag.py / fixtures/tab_drag.json. Source: gui-model/src/tab_drag.rs insertion/chase/100ms gesture logic; gui/src/tab_drag.rs:55 hit testing validates live parent/key and clipped viewport, :150–205 drops stable source into current live destination; notebook_tabs/main Slint routes persistent capture. Model tests/tab_drag.rs:11,72,113 compare staged preference values, reference insertion/Shift outcomes and disable/cancel. GUI tests/tab_drag.rs:56 dispatch real wheel with overflow/selection semantics; :416 delivers actual press/move/release and verifies nested transfer/chase/order/media/disabled/cancel from recording.

**Findings / caveats:** Native scope is in-window transfer only; OS QDrag/window crossing/tree/media/file dragging and empty-notebook body drops are absent. Navigation preference leaves correctly stay Partial. Wheel uses declared 120px increment rather than Qt geometry. Native tests do not assert hidden/accepted-close retained gesture callbacks; tab_drag.rs callback itself has no visible/active guard, so add ownership coverage before expanding lifetime claims. This is not proof of OS event equivalence; render/native run pending.

## tag-list-display-types

- `audit-options-tag-presentation-default-taglist-display-type-advanced-tag-display-type-for-new-page-sidebar-taglists`
- `audit-options-tag-presentation-default-taglist-display-type-advanced-tag-display-type-for-new-media-viewer-taglists`

**Recommendation:** Retain two conditional first_pass opening-default leaves.

**Evidence:** Reference: TagPresentationPanel, ClientGUIListBoxes opening defaults and actual all-known-tags lists in record_tag_list_display_types.py / fixtures/tag_list_display_types.json (eight modes); reference confirms changed defaults leave existing lists unchanged. Source: core/src/tag_presentation.rs typed choices/import, gui/src/page.rs and viewer.rs capture opening mode, store/src/media.rs applies stored/display filters. Model tests/tag_list_display_types.rs:33 checks choices/16 combinations, old payload defaults, Cancel/Apply/reopen. GUI tests/tag_list_display_types.rs:123 compares actual opening sidebar/viewer rows and filters, existing owner retained mode after later Apply (:202–214); :253 asserts independent opening settings.

**Findings / caveats:** Scoped stored mode in actual all-known-tags lists has raw rows; richer service-specific storage decorations and runtime context-menu mode switching remain separate gaps. Existing parent tag presentation remains Partial. Tests/render pending centrally; no unsupported broader claim found.

## thumbnail-cache

- `audit-options-speed-and-memory-thumbnail-cache-memory-reserved-for-thumbnail-cache`
- `audit-options-speed-and-memory-thumbnail-cache-thumbnail-cache-timeout`
- `audit-options-help-debug-action-clear-thumbnail-cache`

**Recommendation:** Retain three conditional first_pass leaves (bytes, timeout, explicit clear).

**Evidence:** Reference: actual Qt BytesControl/time controls, real DataCache actions and actual Help/debug clear publication in record_thumbnail_cache.py:13 / fixtures/thumbnail_cache.json. Source: gui-model/src/thumbnail_cache.rs:70/88 soft-overflow/duplicate admission and touch, :152 strict timeout, :166 policy maintenance; gui/src/thumbnails.rs/grid.rs owns actual decoded buffers/shared cache and generation retirement. Model tests/thumbnail_cache.rs:7 compare every reference cache key/byte/limit event; :67 byte decomposition; :85 concurrent merge; :123 raw timeout preservation. GUI tests/thumbnail_cache.rs:90 verifies staging, real decoded cache bytes, timed expiry, actual Help action, shared page cache, held generations, Cancel/reopen and rebind/accepted exit (:222–278).

**Findings / caveats:** Recorded DataCache synthetic HydrusBitmap sizes test bookkeeping, while native real decoded-buffer checks establish integration. No claims of wider tile/video/image cache completion. Actual native renderer PNG/execution remain centrally pending; no missing claimed consumer boundary found.

## urls-force-metadata-refetch

- `audit-media-context-missing-refetch`

**Recommendation:** Demote first_pass to partial; countAsCompletedLeaf=false.

**Evidence:** Reference: ClientGUIMediaModalActions.py:1192 confirmation and ClientGUI.py:8711 RedownloadURLsForceFetch. Source: GUI thumbnail_menu.rs:1003 builds focused/selected class groups, :1091 question, lib.rs:4988–5045 captures actual URLs and accepted action opens/reuses forced urls downloader with both prefetch flags true. Sole cited thumbnail_menu unit test at :2249 checks menu titles/class rows. Manifest explicitly says no recording and GUI unit test awaits native pass.

**Findings / caveats:** No assertion confirms decline/picker/owner cancellation, selected class URL set, reuse/pending queue, persisted option flags or real downloader skipping recognized URL/hash while fetching metadata. Source presence of both flags is insufficient for claimed producer-to-consumer parity. Add exact reference-backed action and queue/worker regression before counting completion.

## viewing-statistics-options

- `audit-options-file-viewing-statistics-enable-file-viewing-statistics-tracking-in-the-archive-delete-filter`
- `audit-options-file-viewing-statistics-enable-file-viewing-statistics-tracking-in-the-duplicate-filter`
- `audit-options-file-viewing-statistics-min-time-to-view-on-media-viewer-to-count-as-a-view`
- `audit-options-file-viewing-statistics-cap-any-view-on-the-media-viewer-to-this-maximum-time`
- `audit-options-file-viewing-statistics-show-viewing-stats-on-media-right-click-menus`
- `audit-options-file-viewing-statistics-which-views-to-show`

**Recommendation:** Retain six conditional first_pass bounded controls.

**Evidence:** Reference: actual FileViewingStatisticsPanel/time controls, sixteen context-menu cases, eighteen timing controls and 480 actual manager policy outputs in record_viewing_statistics_options.py / viewing_statistics_options.json. Source: gui-model/src/viewing_statistics.rs:18 caps before minimum, gates filter flags, normalizes canvas0; Tracker :91/:120/:146 reads live policy at interval end, keeps same-file start and closes terminally. GUI viewing_tracking.rs connects viewer/archive/delete/duplicate owners; thumbnail_menu views_entries and page sort consume selected canvas/style. Model tests/viewing_statistics.rs assert all policy matrix, interval terminal/last-view semantics and Qt truncation. GUI options_window.rs:4496 replays actual menu labels/style/canvas sums and Cancel; :4664 asserts timed viewer/archive lifetimes; duplicate_filter.rs viewing_statistics_switch tests actual side navigation/closed callbacks.

**Findings / caveats:** The implemented finite controls have real consumers and cancellation/retirement assertions. Native immediate interval writes differ from reference delayed manager flush; explicit API counts remain separately scoped. Preview tick selects stored data without claiming preview canvas. Tracker removes interval before persistence (:92–98) so injected Store failure is not retry-tested; do not claim failure durability. All native/hosted/render results pending.
