## animation-start.json

IDs:

- `audit-options-media-playback-video-animations-start-animations-this-in`

Review source: `3702cdbee99473cfd7af073aa044fe847f78f405`. Read-only source/reference/assertion review; no builds, tests, mutation runs or git/source changes. Hosted exact-source CI and rendered acceptance remain pending centrally. All 25 assigned manifests, including support-only login controls, are covered below.

Retain **Partial / zero completion credit**. The packet already avoids FirstPass despite meaningful implementation. Python `ClientGUICanvasMedia.py:793–829` computes the initial index from the previous widget frame count before replacing metadata. Native `animation.rs::play_with_metadata` (103) follows that order, performs the first seek on the decoder thread and rejects pre-seek generations; an impossible index waits for an explicit seek rather than silently clamping. `tests/model/animation_start.rs:28` asserts recorded raw fractions, Cancel/Apply, truncation on reopen and concurrent policy preservation. Native `animation.rs:443` asserts actual WebP pixels, paused publication, held-frame rejection and explicit recovery; its second reader regression covers ugoira/stop. The claimed supported readers have consumers in viewer/archive-delete/duplicate filters. Unsigned negative-index status, GIF/APNG/JXL/backend breadth and preview animation remain explicit gaps. No further promotion is supported. All native execution/render checks remain centrally pending.

## background-work-timings.json

IDs:

- `audit-options-maintenance-and-processing-deferred-table-delete-idle-ideal-work-packet-time`
- `audit-options-maintenance-and-processing-deferred-table-delete-idle-rest-time-percentage`
- `audit-options-maintenance-and-processing-deferred-table-delete-normal-ideal-work-packet-time`
- `audit-options-maintenance-and-processing-deferred-table-delete-normal-rest-time-percentage`
- `audit-options-maintenance-and-processing-deferred-table-delete-work-hard-ideal-work-packet-time`
- `audit-options-maintenance-and-processing-deferred-table-delete-work-hard-rest-time-percentage`
- `audit-options-maintenance-and-processing-repository-processing-idle-ideal-work-packet-time`
- `audit-options-maintenance-and-processing-repository-processing-idle-rest-time-percentage`
- `audit-options-maintenance-and-processing-repository-processing-normal-ideal-work-packet-time`
- `audit-options-maintenance-and-processing-repository-processing-normal-rest-time-percentage`
- `audit-options-maintenance-and-processing-repository-processing-very-idle-ideal-work-packet-time`
- `audit-options-maintenance-and-processing-repository-processing-very-idle-rest-time-percentage`
- `audit-options-maintenance-and-processing-sibling-parent-sync-processing-idle-ideal-work-packet-time`
- `audit-options-maintenance-and-processing-sibling-parent-sync-processing-idle-rest-time-percentage`
- `audit-options-maintenance-and-processing-sibling-parent-sync-processing-normal-ideal-work-packet-time`
- `audit-options-maintenance-and-processing-sibling-parent-sync-processing-normal-rest-time-percentage`
- `audit-options-maintenance-and-processing-sibling-parent-sync-processing-work-hard-ideal-work-packet-time`
- `audit-options-maintenance-and-processing-sibling-parent-sync-processing-work-hard-rest-time-percentage`
- `audit-options-maintenance-and-processing-sibling-parent-sync-processing-do-work-in-idle-time`
- `audit-options-maintenance-and-processing-sibling-parent-sync-processing-do-work-in-normal-time`

**Demote every listed FirstPass claim to Partial; set countAsCompletedLeaf=false for all 20.** These are persisted controls without corresponding worker behavior. Native `settings.rs:638–681` defines the policy; `options.rs:3190–3308` and `3390–3455` edit it. Repository-wide search for `background_work` finds the options/model schema only; no repository, relationship-sync or deferred-delete runner consumes these timings/switches. The packet itself acknowledges this and absent legacy import. Python `MaintenanceAndProcessingPanel.py:246–264,558–578` edits values that real consumers read: `ClientServices.py:2213–2223`, `metadata/ClientTagsHandling.py:439–449`, `ClientDBMaintenanceManager.py:80–88`. The only cited test, `tests/model/options_dialog.rs::the_options_pages_are_the_references` (508), compares recorded control definitions/defaults; it does not establish consumer pacing, Apply/Cancel/reopen of these settings or import. Saving an inert preference does not complete the corresponding reference leaf. This is an accounting overclaim, not a claim that existing synchronous writes need artificial sleeps.

## database-file-history.json

IDs:

- `audit-media-menu-database-view-file-history`

Retain **Partial / zero completion credit**. Python recorder/fixture `record_file_history.py` / `file_history.json` establish the four sampled cumulative series and chart range/visibility rules. Native `file_history_window.rs` owns a private chart/query; `hydrus-search/src/exec/mod.rs:128` supplies a history-specific query bypassing implicit caps while keeping explicit limits. `tests/model/file_history.rs:19` compares actual series and range transitions; :139 and :235 cover coalescing, cancelled/closed and dropped workers. `tests/gui/file_history.rs:90–178` asserts uncapped matching imports, unchanged main session, predecessor/hidden-owner rejection and genuine fallible startup followed by worker reuse. The worker cancellation path is substantive source evidence. Full history search widget/chart topology remains incomplete as the packet states; child support must not become separate completion credit. Central native execution/render is pending.

## database-vacuum.json

IDs:

- `audit-media-database-maintenance-vacuum-run`

**Demote to Partial / zero completion credit pending confirmation ownership repair. [P2] Native acceptance is not tied to a pending live question.** `vacuum_review_window.rs:166–176` calls global `close()` and `run(store.clone())` on every true answer, even with no selected eligible row, no question, a hidden/closed window or a retained predecessor. `close()` (73) clears thread-local OPEN, so an old answer can close a newly opened successor and vacuum the old captured Store. Repeated true answers start repeated workers. The window is outside the owner-local slots used by recent maintenance dialogs. Python `ClientGUIScrolledPanelsReview.py:2260–2281` derives selected names, obtains its modal answer, then writes the selected vacuum command once. Native Store `vacuum.rs::vacuum` does execute real VACUUM and records timestamps; :tests asserts freed pages and subsequent Store reads. `tests/model/vacuum_review.rs` asserts hand-authored row/estimate strings but loads no reference fixture. There is no cited native confirmation, decline, double-accept, hidden/main-close/rebind, failure or reopen test. The packet has no recorded oracle, so its exact GUI contract is additionally under-evidenced. One-file/in-place VACUUM is explicitly disclosed, not a separate defect. CI/render remains pending.

## deferred-delete-menu-switches.json

IDs:

- `audit-media-menu-database-work-deferred-delete-jobs-during-idle-time`
- `audit-media-menu-database-work-deferred-delete-jobs-during-normal-time`

**Demote both FirstPass claims to Partial / zero completion credit.** `main_menu.rs:1059–1069` renders saved ticks; `menu_bar.rs:800–815` toggles `BackgroundWork` fields; `settings.rs:656–675` supplies defaults. These fields have no deferred-delete consumer anywhere in native crates. Python `ClientGUI.py:3347–3359` exposes matching switches, and `ClientDBMaintenanceManager.py:30,42` reads them to decide whether deletion may work. The packet's own limit explicitly says “No consumer”. `tests/model/main_menu.rs` provides menu composition evidence, not a real native toggle/cancel/failure/reopen or worker gating replay. This is supported partial menu plumbing, not completed reference behavior.

## duplicates-progress-label.json

IDs:

- `audit-options-duplicates-duplicates-filter-page-hide-the-x-done-notification-on-preparation-tab-when-99-searched`

Retain the **conditional FirstPass proposal for the single listed ID**. Python `ClientGUISidebarDuplicates.py:452–510` has strict >99% suppression and the pending 100.0%→99.9% label. Native `duplicates_page.rs`/`duplicates_sidebar.rs` now read the saved typed policy, rather than a constant. `tests/model/duplicates_progress.rs:24` replays both Boolean settings over the recorded count boundaries and preserves concurrent preferences; Store `duplicates_progress.rs` covers imported false/native override/corruption/reopen. `tests/gui/duplicates_progress.rs::actual_checkbox_relabels_visible_preparation_without_changing_work_and_future_pages_reopen_policy` asserts draft isolation, hidden/retired Apply, actual current labels, unchanged can-start/count/gauge, future pages and reopened saved policy. The actual Qt `duplicates_progress_option.json` publisher uses held maintenance numbers; this is adequate for a finite display-control leaf, not proof of the preparation worker. No broad duplicate/preparation parent credit. Central native execution/render remains pending.

## file-maintenance-current.json

IDs:

- `audit-media-database-maintenance-current`

Retain **Partial / zero completion credit**. `file_maintenance_current.rs::Control::bind` (214 onward) owns a serial background command actor and weak polling timer, while `retire` stops admission/wakes the receiver. Physical runners and persisted queue consumers are real. `tests/model/file_maintenance_current.rs:6` compares recorded counts/due gates/sorting/typed selection. `tests/gui/file_maintenance_current.rs:95,183,211` covers clear/force/menu, held-lease cancellation and redownload dispatch; :263 verifies captured Clear is serviced before the next physical batch and prevents the physical metadata runner from overwriting the cleared EXIF work. :330 covers exit decline/accept; :380/:424 cover final-binding retirement and pre-work gauges. Qt `file_maintenance_current.json` is scoped scheduled-work evidence. Refresh during a physical batch, full-pass independent-process lease granularity and waiting-job timing are still gaps; the packet correctly retains Partial. No new completion credit; central CI/render pending.

## folders-export.json

IDs:

- `audit-network-tagging-regex`
- `audit-network-tagging-simple`
- `audit-network-import-folder-log`

Retain the scoped filename-regex and simple-tag statuses and the **support-only/zero-credit import-folder-log claim**. The regex/simple IDs already occur in `overnight/progress.json`, so both earn **zero additional credit in this pass**, even though this packet still has countAsCompletedLeaf:true. `tests/model/filename_rules.rs:6` replays real quick/regex Add/Edit/Delete/sorting, invalid patterns, draft isolation and extracted tags from recorded filenames. `tests/gui/filename_rules.rs:24,282` verifies real folder dialogs, frozen child/service identity, parent transaction and regex menus/favourites. `tests/gui/filename_simple.rs:44,300` covers actual selection/paste/removal reaching manual import seeds and folder children; `tests/gui/import_files.rs:198` checks imported tags. The log consumer `folders_window.rs:256` installs full seed drafts transactionally; `tests/gui/import_folder_log.rs:73,285` verifies child Apply/Cancel versus folder/manager Cancel, intentional retry hash scrub, next worker seed and retained callbacks. Manager lifecycle source uses OS activity/edit leases; `folder_manager_lifecycle.rs:187,251` asserts read-after-worker-commit and genuine read/save failure release, and CLI folder_wait tests cover wakeups. This is not broad export-folder/query/sidecar parity. Central native/render remains pending.

## gui-format.json

IDs:

- `audit-options-gui-misc-prefer-iso-time-2018-03-01-12-40-23-to-5-days-ago`
- `audit-options-gui-misc-experimental-bytes-strings-1kb-pseudo-significant-figures`

Retain **conditional FirstPass proposals for the two IDs**. The source provides owned formatting rather than a mutable global: `gui_format.rs:17–43` applies the current UTC offset to target dates, matching recorded Python behavior, and selected consumers call explicit precision-aware APIs. `tests/model/gui_format.rs:25,139,167,277` checks staged controls, recorded offsets/ancient boundaries, byte rounding and actual model consumers. `tests/gui/gui_format.rs:47` replays Cancel, stale Apply, saved/reopened controls and real file/gallery rows/page total; backend `hydrus-net/src/engine.rs::formatting_refresh_keeps_a_bandwidth_wait_and_its_usage_and_cancel_reason` (2376) asserts unchanged accounting/job state and cancellation across reload. Actual backend oracle `gui_format_backend.json` includes wait thresholds and import size diagnostics; native/import/net tests exercise those consumer boundaries. Already painted cached labels use ordinary refresh paths, and immediate global rebroadcast is deliberately not claimed. Whole historical/platform date coverage and integer locale remain outside the two leaves. Central execution/render pending.

## image-cache.json

IDs:

- `audit-options-speed-and-memory-image-cache-memory-reserved-for-image-cache`
- `audit-options-speed-and-memory-image-cache-image-cache-timeout`
- `audit-options-speed-and-memory-image-cache-maximum-image-size-in-of-cache-that-can-be-cached`

Retain **three conditional FirstPass proposals**. Python `ClientCaches.py:166–203` and `ClientCachesBase.py:177,227,266,304` establish strict single-image admission, access-updated footprints, soft overflow and strict idle expiry. Native model `image_cache.rs:36–75` has those rules over the shared LRU; native `image_cache.rs::load_using/render_using` captures ICC/resolution identity and refuses retired/changed-policy admission. Real preview, still viewer, archive/delete and duplicate-filter consumers use that shared cache. `tests/model/image_cache.rs:11,76,142,194` checks recorded pending/loaded byte transitions, strict equality/expiry, legacy/imported settings and scoped concurrent save. Native in-module tests cover failed/replaced/retired pending decodes; `tests/gui/image_cache.rs:81` verifies actual current pixels/viewing intervals survive shrink and final Bound retirement. Percentage affects future admissions, not arbitrary current canvas clearing. The new owned cache is per Main incarnation rather than Python's controller-global cache, which is an explicit design caveat. Prefetch/tile/video/global-cache parents are not complete. Central runtime/render pending.

## login-script-controls.json

IDs:

- `login-script`
- `login-scripts`

Retain **support-only Partial / zero completion credit** for login-script and login-scripts; claims[] is intentionally empty. `login_test_window.rs::RunSlot::review/command` (188/202) qualifies fresh temporary-engine epochs with a unique per-run generation before forwarding controls, so restarted request IDs cannot alias. `login_script_controls.rs::Binding::information` (289) owns the exact completion notice and waits for acknowledgement before the owner's final state; local Help resolves existing downloader_login.md. `tests/gui/login_script_controls.rs:16` asserts show/copy/clear errors, sole OK notice, hidden-owner refusal, acknowledged-once, parent Cancel and stale predecessor preservation; :158 exercises real isolated job controls. `tests/gui/login_workflows.rs:627` uses real loopback HTTP for test completion, preserving source cookies. `hydrus-net/src/engine.rs` exempts login startup admission while keeping ordinary response-byte accounting/throttling; this source support is narrower than whole native network parity. Qt/network/error and platform boundaries listed in the packet remain unclaimed. No new IDs/status promotion and central CI/render pending.

## manage-tags-sort.json

IDs:

- `audit-options-tag-sort-tag-sort-default-tag-sort-in-search-page-manage-tags-dialogs`
- `audit-options-tag-sort-tag-sort-default-tag-sort-in-media-viewer-manage-tags-dialogs`

Retain **two conditional FirstPass proposals**. Python TagSortPanel.py:53/55 defaults are independent for search-page versus viewer Manage Tags; actual Qt `manage_tags_sort.json` records 72 sorting combinations and dialog lifetimes. Native `manage_tags.rs::new_at` captures the correct context; `plain_rows` (353) uses captured/local control, effective sibling keys only when sibling information is shown, and raw logical storage tags/counts for edits. `tests/model/manage_tags_sort.rs:107` asserts independent staged Cancel/Apply/reopen and concurrent scoped save; :170 compares every recorded ordering, :195 preserves per-service local sort across external Options changes, :231 tests sibling-disabled raw keys. `tests/gui/manage_tags_sort.rs:122,211,305` exercises real Options, search selection versus viewer current-file callers, retained sorting children and main close/rebind retirement. These consumers support the finite defaults; broader Manage Tags remote/petition/context-menu/count-display limits remain Partial. Central native/render pending.

## media-options.json

IDs:

- `audit-options-media-playback-zoom-and-position-re-center-media-on-window-resize`
- `audit-options-media-playback-transparency-draw-image-transparency-as-checkerboard`
- `audit-options-media-playback-transparency-instead-of-checkerboard-use-a-bright-greenscreen`
- `audit-options-media-viewer-animation-audio-seek-bar-seek-bar-height`
- `audit-options-media-viewer-animation-audio-seek-bar-seek-bar-height-when-mouse-away`
- `audit-options-media-viewer-animation-audio-seek-bar-seek-bar-nub-width`

Retain all **six existing finite statuses**, with **zero additional completion credit**: all IDs are already in `overnight/progress.json`. Qt `viewer_canvas_options.json` records actual panel Apply, resize, transparency and seek geometry/targets. Native `viewer_presentation.rs::refresh` applies saved checkerboard/greenscreen and seek geometry; `lib.rs:6320` supplies a live saved recenter policy to zoom. `tests/model/options_dialog.rs:1303` verifies defaults/bounds/None/draft isolation; `tests/gui/options_window.rs:2044,2193` verifies Cancel, actual viewer refresh, preserved pan versus recenter, pixel-level seek height/nub extent and hidden-None behavior. `tests/gui/scanbar.rs::configured_nub_width_seeks_the_recorded_animation_frames` asserts actual mouse→frame targets. Legacy conversion regression exists. Native layout/5px hidden-ideal discrepancy is explicitly disclosed; no backend-wide animation/audio completion. Failure injection for saving these independent low-risk settings is not feature-specific in this packet. Central current-source runtime/render pending.

## namespace-colour-controls.json

IDs:

- `audit-options-tag-presentation-namespace-colours-add`
- `audit-options-tag-presentation-namespace-colours-delete`
- `audit-options-tag-presentation-other-rendering-namespace-for-the-or-top-row`

Retain **three conditional finite FirstPass proposals**. Actual Qt `namespace_colour_controls.json` records normalization, protected/default deletion, Shift/Ctrl+Shift contractions, Warning/OK ownership and OR namespace resolution. Native `namespace_colours.rs:70` follows lowercase/Python whitespace/trailing-colon/gumpf cleanup and retains numeric range bookkeeping while remapping selected identities; removal retains both protected defaults. `options_namespace_colours.rs` owns Add/Delete/warning children and blocks parent Apply. `tests/model/namespace_colours.rs:20,141` replay normalization and sorted selection, and `tests/gui/namespace_colours.rs:62` asserts exact questions/OK notices, real LineEdit Return, no-op protected deletion, hidden/stale child refusal, persisted/reopened colour maps and actual OR/tag-row colour consumers with unchanged queries. OR None versus explicitly edited empty string is tested. Full colour picking/menus/expanded OR child topology is broader uncompleted work; source adds later editor support without credit here. Native CI/render pending.

## open-externally.json

IDs:

- `audit-options-open-externally-url-calls-add`
- `audit-options-open-externally-url-calls-edit`
- `audit-options-open-externally-single-file-calls-add`
- `audit-options-open-externally-single-file-calls-edit`
- `audit-options-open-externally-single-file-calls-delete`
- `audit-options-nested-open-file-call-list-choose`
- `audit-options-nested-open-file-call-list-add-edit`
- `audit-options-nested-open-file-call-list-order`

Retain **eight conditional finite FirstPass proposals**. Qt OpenExternallyPanel.py list/nested handlers and `open_externally.json` establish registered choice filtering, missing identities, ordered defaults, MIME override/General File protection and nested Cancel. Native `options_open_externally.rs::queue_action/open_files/file_action` owns private ordered child queues; `open_externally_launch.rs:101` resolves current saved registered calls and actual local-storage membership at dispatch. `tests/model/open_externally.rs:38,101,165` compare registered options, washing/renames and empty-specific fallback. `tests/gui/open_externally.rs:149,377,527` replay Options/nested Apply/Cancel/parent block, protected General File, reopened vectors and real selected/viewer launches; asserts missing-call exact Information, live preference updates, retained-path rejection after deletion, hide/rebind/accepted-exit retirement. Process-call failure is handled asynchronously by the owner. Launchers use a test seam for OS side effects; no claim of exhaustive platform process behavior. Broader route context menus/pipelines remain Partial. Central native/render pending.

## popup-job-actions.json

IDs:

- `audit-options-popups-clipboard`
- `audit-options-popups-callable`
- `audit-options-popups-yes-no`

Retain **three conditional FirstPass proposals**. Actual Qt `popup_actions.json` establishes producer-labelled clipboard/callable/question controls and cutoff/paused visibility. Native `popup_job_actions.rs:22–130` validates GUI owner and current Store producer/question tokens; Store `popup_actions.rs` atomically rejects replaced/double answers. Download Working producers own repeatable executable callbacks; persisted API/GUI intents avoid serializing code. `tests/gui/popup_job_actions.rs:19,150,217,265,325` compares actual payloads/actions, stale/replaced controls, retained owner/rebind and final-clone Drop; :325 asserts independent producer remains usable after GUI retirement. Current `lib.rs:3314` does compose popup_timer.retire_callback into accepted-close cleanup, satisfying the packet's integration requirement; exit Cancel remains live. Authenticated API assertions are in `hydrus-api/tests/popup_actions.rs`. Stock producer breadth, modal/freeze and interruption of already started effects remain outside these leaves. Central runtime/render pending.

## radio-return.json

IDs:

- `audit-options-gui-misc-force-that-hitting-enter-return-on-radio-button-lists-triggers-a-dialog-ok`

Retain **one conditional FirstPass proposal**. Actual Qt `radio_return.json` records all 40 Linux/offscreen Enter/Return cases: false delegates to the parent default rather than guaranteeing suppression. Native filesize/hash/deletion radio handlers read saved policy on each key; `predicate_editor_window.rs::on_force_radio_ok` validates visible owner, panel kind and active child/notices. `tests/gui/radio_return.rs:144` establishes actual radio keyboard focus with arrow movement then dispatches Return under live saved policy, comparing dialog acceptance/predicate result; deletion and stale/hidden-child tests exist at :305/:408. `tests/model/radio_return.rs:32` and Store `radio_return.rs` cover staged Apply/Cancel/reopen/native-wins legacy values. Multiline hash input keeps newline consumption. This is every currently represented radio group, with reference platform/physical-key translation limits explicitly disclosed. No broader GUI radio family completion. Central native/render pending.

## search-options.json

IDs:

- `audit-options-file-search-file-search-autocomplete-start-new-search-pages-in-searching-immediately`
- `audit-options-file-search-file-search-autocomplete-show-system-everything`
- `audit-options-file-search-file-search-autocomplete-default-fallback-local-file-search-location`
- `audit-options-file-search-file-search-autocomplete-active-search-predicates-list-height`
- `audit-options-file-search-file-search-autocomplete-autocomplete-list-height`
- `audit-options-file-search-file-search-autocomplete-autocomplete-dropdown-floats-over-file-search-pages`
- `audit-options-file-search-file-search-implicit-system-limit-for-all-searches`
- `audit-options-file-search-file-search-if-explicit-system-limit-then-refresh-search-when-file-sort-changes`

Retain all **eight existing finite statuses**, with **zero additional credit** because all eight are in `overnight/progress.json`. Actual Qt defaults/location/presentation/limits fixtures cover staged preferences and consumer semantics. `tests/gui/options_window.rs:1602,1716,1834,1954` exercises new-page synchronized state, domain fallback and child Cancel, embedded/floating actual geometry, shared search caps and limited sort refresh. Model :1098/:1149/:1192/:1252 checks default/range/private drafts. `hydrus-search/tests/file_search_options.rs:12` runs real imported database queries and compares returned hashes for implicit versus explicit limits; its second test compares every recorded database-sort eligibility/location. Native `page.rs::sort_changed_search_or_resort` (2172) gates reruns on synchronization/context/sort. Controls' saved values reach actual constructors/queries. Direct importable-location selector and native font-row geometry differ from Qt and are declared. Existing GUI ownership integration remains relevant; these fixture tests do not establish all autocomplete/search parents. Central runtime/render pending.

## shortcut-capture.json

IDs:

- `audit-options-nested-shortcuts-command-keyboard`
- `audit-options-nested-shortcuts-command-mouse`

Retain **keyboard conditional FirstPass** and **mouse Partial / zero completion credit**. Actual Qt `shortcut_capture.json` records key kinds/modifiers/casefold/numpad and mouse-wheel semantics. Native `shortcut_capture.rs::Capture` keeps keyboard/mouse drafts separate, filters capture identity and uses saved policies; `shortcut_input.rs` converts raw backend key location/modifiers to the same model. `tests/model/shortcut_capture.rs` replays exact recorded gestures/text, including keypad alternate policy. `tests/gui/shortcut_capture.rs` asserts command→set→Options staging, Cancel/stale child rejection, saved main/viewer navigation and permanent close/rebind ownership; raw backend key identity is checked explicitly. Mouse's 400ms/5px double-click fallback, PixelDelta borrowing and simultaneous-button/platform ownership remain disclosed unproven gaps, so the model replay cannot justify mouse FirstPass. Capture supports only native-dispatched commands; broader shortcut command breadth remains Partial. Central native/render pending.

## sidebar-layout.json

IDs:

- `audit-options-sidebar-splitters`
- `audit-options-menu-menu-pages-show-hide-sidebar-and-preview-panel`
- `audit-options-menu-menu-pages-save-current-page-s-sidebar-preview-size-on-client-exit`
- `audit-options-menu-menu-pages-save-current-page-s-sidebar-preview-size-now`
- `audit-options-menu-menu-pages-restore-all-pages-sidebar-preview-sizes-to-saved-value`

Retain **five conditional finite FirstPass proposals**. Actual Qt `sidebar_layout.json` and hidden-page-preview fixtures cover splitter/menu dimensions and preview-retention semantics. Native `sidebar_layout.rs::State` keys geometry to stable PageKey plus weak SearchPage incarnation, guards visibility/epoch on resize and persists actual measured dimensions; Toggle clears only preview focus. `tests/gui/sidebar_layout.rs:95` uses actual pointer gestures, menu commands, per-page retention, hidden-zero saves, restore-all and accepted/declined exit. :321 proves accepted hidden preview survives live hide-policy changes and collapse preserves selection/viewing accounting; :493 rejects held old handle against rebind/same-key session reload and checks oversized legacy positions. `page_layout` Store/model tests cover durable defaults/reopen. Native six-pixel handles/minimum-reserve differ from Qt, explicitly disclosed. No unrelated session/splitter parent credit; central native/render pending.

## subscription-merge.json

IDs:

- `subscriptions-merge`

Retain **conditional FirstPass for subscriptions-merge**. Python `ClientGUISubscriptions.py:3360–3477` builds all compatible groups, aborts every pending merge on primary cancellation, treats name cancellation as unchanged name, removes absorbed owners before allocating names and commits replacements together. Native `subscriptions_dialog.rs::merge_many` (934) follows those identity/name rules and moves entire query drafts. `tests/gui/subscriptions.rs:635` replays all recorded questions, asserts unchanged draft rows and Store throughout pending decisions, compares primary IDs/settings and full query histories after list Apply, and rejects closed callbacks against successor. :816 covers parent Cancel of pending/completed drafts. Bulk save moves queue owners before deleting subscriptions. The packet correctly makes no forced unreachable no-eaten warning claim. Manager/daemon-wide editing lock and other subscription boundaries remain separately Partial, not proven by this finite transaction leaf. Central runtime/render pending.

## tag-dialog-preferences.json

IDs:

- `audit-options-tag-editing-tag-dialogs-use-listbook-instead-of-tabbed-notebook-for-tag-service-panels`
- `audit-options-tag-editing-tag-dialogs-show-parent-info-by-default-on-edit-write-taglists`
- `audit-options-tag-editing-tag-dialogs-show-parents-expanded-by-default-on-edit-write-taglists`
- `audit-options-tag-editing-tag-dialogs-show-sibling-info-by-default-on-edit-write-taglists`

Retain **four existing finite FirstPass statuses**, with **zero additional credit** because all four IDs already appear in `overnight/progress.json`. Qt `tag_dialog_preferences.json` records the defaults/service topologies and raw-storage parent/sibling decorations. Native ManageTags captures opening defaults; `manage_tags.rs::display_rows` renders implied parent rows/suffixes and maps inherited activation back to the originating storage tag. Shared `tag_service_panels.slint` supports both real horizontal/vertical service navigation in Manage Tags and relationships. `tests/model/tag_dialog_preferences.rs:54,115` compares all combinations and preserves unrelated autocomplete defaults. `tests/gui/tag_dialog_preferences.rs:86,175` verifies Options Apply/Cancel, actual laid-out content offsets/topologies, service-specific autocomplete drafts, mapping Cancel/Apply, persisted reopening and new dialogs reading defaults while old drafts survive. Parent order and all-file count elision/remote petition work are explicit broader gaps. Central runtime/render pending.

## thumbnail-appearance.json

IDs:

- `audit-options-thumbnails-appearance-fade-thumbnails`
- `audit-options-thumbnails-appearance-use-blurhash-missing-thumbnail-fallback`
- `audit-options-thumbnails-media-background-experimental-image-path-for-thumbnail-panel-background-image-set-blank-to-clear`
- `audit-options-thumbnails-new-rendering-tech-use-the-new-thumbnail-rendering-tech-only-applies-to-new-pages`

Retain **two conditional finite FirstPass proposals** (blurhash and background path), and **Fade/new renderer Partial / zero credit**. Qt `thumbnail_appearance.json` includes actual old and inherited default-new viewport backgrounds, path picker semantics and byte-exact blurhash recovery; the new renderer flag is creation-time only. Native `thumbnail_background.rs:8` caches by literal path; :27 seeds single-file Browse, normalizes lexically and preserves Cancel. Loader recovery copies admitted policy/geometry, and `hydrus-media/src/blurhash.rs:145` implements reference base83/gamma/components plus 32×32 wrapper/resizing. Its fixture test compares actual recovery bytes; `tests/gui/thumbnail_appearance.rs:164` uses real missing physical original/thumbnail with valid blurhash and asserts visibly different disabled pixels and stale policy rejection. :396/:650 verifies unscaled nonuniform 31×17 extent/corners/resizing/clipping/scroll/background clear and live-owner restrictions. Current colour integration supplies copied semantic thumbnail palette roles; no extra colour-control credit arises here. Opacity per primitive differs from Qt composed QPixmap and native grid is not Qt renderer architecture, so those two Partial assessments are correct. Central render is mandatory and pending.

## unselected-tag-cap.json

IDs:

- `audit-options-tag-presentation-selection-tags-max-number-of-thumbnails-to-compute-tags-for-when-none-are-selected`

Retain the **existing finite FirstPass status**, with **zero new credit** because this ID is already in `overnight/progress.json`. Python `ClientGUIMediaResultsPanel.py:1166–1193` slices sorted thumbnail items before flattening collections and bypasses the cap on any selection. Native `page.rs::tag_computation_limit/tag_list_title/count_tags` (2812/2816/2879) implements that item-prefix contract and saved sidebar captions. `tests/gui/unselected_tag_cap.rs:124` compares real page collections/hash groups, cap notice, painted tags/title, zero/None/finite values, selected-last/all bypass, parent Cancel, saved/reopened settings and background-page reactivation; :243 verifies sorting changes the actual prefix. Model :10 and import conversion regression cover default/bounds/legacy values. This evidence supports the specific cap, not full Tag Presentation/tag-list interaction. Central runtime/render pending.

## viewing-statistics-cleanup.json

IDs:

- `audit-media-menu-database-clear-clear-all-file-viewing-statistics`
- `audit-media-menu-database-clear-cull-file-viewing-statistics-based-on-current-min-max-values`

Retain **two conditional FirstPass proposals**. Python `ClientDBFilesViewingStats.py:55,65` clears globally or validates both canvas min/max sets before minimum-count reduction and maximum-duration capping. Native `viewing_maintenance.rs:12,29` performs actual SQL inside one writer transaction, preserving other canvases/timestamps; zero/None rules and invalid bounds are covered by model reference replay. `viewing_maintenance_window.rs:15` owns one visible confirmation, accepted-once flag and timer closure; operation reads current saved limits at acceptance. `tests/gui/viewing_maintenance.rs:57` asserts exact questions/decline preservation/current-rule changes and persisted reopened records; :145 covers hidden/stale/successor owners, :200 checks closing/dropping releases the timer owner. Unlike the vacuum window, this callback checks ownership and accepted-once before mutation. Native invalid rules use an owned Warning instead of Python's global exception presentation, explicitly disclosed. Preview min/max controls support culling but do not prove timed preview viewing. Central runtime/render pending.
