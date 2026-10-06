# Batch 2 read-only manifest review

Inspected source checkpoint: `3702cdbee99473cfd7af073aa044fe847f78f405` in `/workspace/hydrus-mine`. Read AGENTS.md, architecture/conventions/roadmap, relevant GUI/DIFFERENCES and oracle/coverage guidance. No builds, tests, mutations or source/Git edits were performed. Findings distinguish source-demonstrated defects from missing regression evidence. All retained first-pass items are conditional proposals; central exact-source CI/platform/runtime and native rendered review are pending. No fully validated completion is attested. Machine-readable demotions are in `demotions-2.json` and exclude all IDs already in the canonical 240/51 completed arrays. Prior validated IDs in this batch (gallery-source, media-focus, local-service deleted and twelve tag-autocomplete leaves) retain their prior ledger standing; inspection here does not retroactively claim a new validation result. Paths below are repository-relative; line numbers cite this reviewed checkpoint unless explicitly historical.

## active-predicate-edit

**Recommendation:** Retain Partial / zero completion credit.

**IDs:** `audit-options-search-active-edit`

Reference: `ClientGUIACDropdown.py:3195` edit set differences, `:3378` add inverses and `ClientGUISearch.py:69,195`; `active_predicate_edit.json` records 13 commands and 12 supplied-value editor paths. Native `src/active_predicates.rs:117` gates Main input; its captured page/predicate state and `predicate_editor_window.rs` active/exact-owner cleanup protect replacement. `tests/model/active_predicates.rs:46,93` compare command result sets and existing values against the recording; `tests/gui/active_predicates.rs:61,157,237` exercise supplied-size Cancel/accept/query changes, switched/hidden/rebound owners and final-owner release. Current source additionally has mixed/routes implementation from later packets, but this packet's explicit Partial0 assessment is conservative. Do not credit the broader batch editor/menu routes from this packet alone. Hosted execution/render remains pending.

## auto-resolution-rules-exchange

**Recommendation:** Retain Partial / zero completion credit.

**IDs:** `audit-media-rules-exchange`

Reference `ClientGUIDuplicatesAutoResolution.py:77,239` installs list exchange and makes imported rule names unique. Native `auto_resolution_exchange.rs` accepts native JSON and legacy rule/list tuples; `auto_resolution_rules_window.rs` exposes clipboard/file/PNG transports. `tests/model/auto_resolution_exchange.rs:10,21` assert native round-trip, malformed text, the checked-in suggested reference rules and wrong types. Those assertions do not cover native picker cancellation, partial multi-file failure, owner retirement, naming collisions or saved/reopened imported rules. The packet correctly remains Partial, but its behavior text should not imply those boundaries have been replayed. Native exports remain unreadable by Python and comparator exchange/clipboard image are documented missing (`DIFFERENCES.md:3456`).

## database-archive-times

**Recommendation:** Retain conditional first-pass proposal for the one exact action.

**IDs:** `audit-media-menu-database-fix-missing-file-archived-times`

Reference `record_archive_time_repair.py` and `archive_time_repair.json` capture real warning/scan/choice cancellation, strict tracking boundary, current/trash/former-local candidates, both populations and fresh-client timestamps. Native store `archive_repair.rs:49,113` scans and revalidates captured candidates inside a content transaction, polling cancellation before/between writes. `tests/model/archive_repair.rs:60,138` replay exact questions/plans and preservation of concurrent archive/inbox/import edits. `tests/gui/archive_repair.rs:50` asserts menu dispatch, declined and cancelled owners, successor slot preservation, actual saved times, changed callback and reopened no-work result. No source defect found in this bounded inspection. `countAsCompletedLeaf` is absent, so do not infer additional credit; only this exact first-pass action is eligible after central verification. Large-client scan/performance and final native render remain unverified.

## database-maintenance

**Recommendation:** Demote all 18 first-pass/countAsCompletedLeaf claims to Partial / zero completion credit until repaired and covered.

**IDs:** `audit-media-menu-database-analyze`, `audit-media-menu-database-clear-fix-orphan-file-records`, `audit-media-menu-database-clear-orphan-url-mappings`, `audit-media-menu-database-clear-orphan-tables`, `audit-media-menu-database-get-tables-using-definitions`, `audit-media-database-repair-invalid-tags`, `audit-media-menu-database-check-and-repair-fix-logically-inconsistent-mappings`, `audit-media-menu-database-check-and-repair-resync-tag-mappings-cache-files`, `audit-media-menu-database-regenerate-tag-storage-mappings-cache-all-with-deferred-siblings-parents-calculation`, `audit-media-menu-database-regenerate-tag-storage-mappings-cache-just-pending-tags-instant-calculation`, `audit-media-menu-database-regenerate-tag-display-mappings-cache-all-deferred-siblings-parents-calculation`, `audit-media-menu-database-regenerate-tag-display-mappings-cache-just-pending-tags-instant-calculation`, `audit-media-menu-database-regenerate-tag-display-mappings-cache-missing-file-repopulation`, `audit-media-menu-database-regenerate-tag-siblings-lookup-cache`, `audit-media-menu-database-regenerate-tag-parents-lookup-cache`, `audit-media-menu-database-regenerate-tag-text-search-cache`, `audit-media-menu-database-regenerate-tag-text-search-cache-subtags-repopulation`, `audit-media-menu-database-regenerate-tag-text-search-cache-searchable-subtag-maps`

Reference `record_database_maintenance.py`/`database_maintenance.json` capture real questions/service choices and successful popups on a healthy basic client. `tests/model/database_maintenance.rs:38` checks recorded refusal writes, not a native refusal; `:127` invokes model `run` directly with default all-service/hash/soft answers and compares popups. Store `db_maintenance.rs:480,541` covers healthy regeneration and seeded mapping/orphan repairs, but the native dispatch has no behavioral regression. Native `database_maintenance_window.rs:52` clones Slot and Store into the question callback: Slot.question -> window -> closure -> Slot.question is a strong cycle. Cancel/No merely hide (`:65`) without taking the slot, so the cycle can retain Store after its visible owner closes. A retained cancelled/hidden question can later answer Yes (`:60`) and launch maintenance; a late old question may replace a successor chooser (`:151`). Choice answers also lack parent activity/identity/visibility checks (`:85,109,145`). This is a demonstrated source path, distinct from absent tests. Jobs cannot be cancelled after start by documented scope; the missing cancellation is before admission. Failure leaves a titled working popup unfinished (`model/database_maintenance.rs::run` early `?` before finish) plus the error text popup; no test covers failure. Per-service cache selection/Analyze full/tag-definition selection also lack accepted native coverage. Native schema equivalents and no-progress policy are documented at `DIFFERENCES.md:3410` and can remain scoped differences.

Minimal centralized repair: make Slot ownership weak in callback closures; capture one exact dialog/chooser identity and generation per admission; require child visible + current slot + live Main/Bound before accepting or replacing; retire/clear exact slots on No, Cancel, X, accepted close, rebind and final owner drop. Preserve the explicitly documented policy for already-admitted background jobs. Meaningful native regressions: seed orphan URL/inconsistent mappings, exercise No/Cancel/X and verify rows unchanged; retain A, cancel then open B, invoke/show A and verify no writes/no successor displacement; hide child/Main, reject acceptance; admit a live answer exactly once, verify transaction/results and reopen; exercise service chooser Cancel and stale choices; assert weak Store releases after Bound/window/collector drop. Use real rows/popups/clipboard outcomes, not callback-presence checks.

## debug-long-popup

**Recommendation:** Retain conditional first-pass proposal for the one exact debug action.

**IDs:** `audit-options-help-debug-action-make-a-long-text-popup`

Python `ClientGUI.py:1355,1376,3639` supplies the five-word body/title schedule and menu action; `debug_long_popup.json` records every deadline and exact durable string. Native `debug_long_popup.rs:49` live checks, `:52` retirement, `:64` weak timer and `:93` bounded publication separate hidden ongoing work from launch eligibility. `tests/gui/debug_long_popup.rs:68` compares every before/at deadline job and card against recorded strings/flags, persists/reopens and checks changing pixels/caps. `:159` covers dismissal and overlapping jobs; `:202,306` cover hidden launch, exit Cancel/accept, rebind, Main loss and Bound clone lifetime. Timing injection controls native scheduling for replay; it does not prove real OS delivery latency or font geometry. No defect found in scoped source review; pending central runtime/render is still required.

## duplicates-filtering

**Recommendation:** Demote four first-pass/countAsCompletedLeaf claims to Partial; retain existing count Partial0.

**IDs:** `audit-media-filter-sidebar-sort-group`, `audit-media-filter-sidebar-pixels`, `audit-media-filter-sidebar-random`, `audit-media-filter-sidebar-search`, `audit-media-duplicate-search-count`

Reference `ClientGUISidebarDuplicates.py:94,114,184` defines group mode/random display; `ClientGUIPotentialDuplicatesSearchContext.py` defines search/pixel controls. The cited `tests/model/duplicates_filtering.rs:34` uses hardcoded expected strings, kind toggle and parser operations; `:65` only asserts matching <= total and total==0 OR group>=2, so an empty basic fixture supplies no positive random-result proof. There is no recording replay, actual filter pair ordering/group/pixel assertion, native Cancel/consumer or persisted page round-trip in this packet. Native callbacks change d.order/group/pixel/search, but presence alone does not prove the four leaves. Concrete adjacent defect: `duplicates_filtering_sidebar.rs:87` publishes a completed old count directly to Main/global COUNT without captured page/search/binding generation; overlapping jobs or page changes can display another page's stale count. `:235` relationship question accepts captured files after cancel/retirement and has no exact question slot guard. These defects must remain visible even though the count family already has Partial status. Repair with captured page key/search generation and weak exact question owner; regress old/new count ordering and retained question after page close/rebind.

## ffmpeg-timeout

**Recommendation:** Retain conditional first-pass proposal for the exact timeout control.

**IDs:** `audit-options-media-playback-system-ffmpeg-call-timeout`

Python `MediaPlaybackPanel.py`, `HydrusFFMPEG.py` and `HydrusSubprocess.py` plus `ffmpeg_timeout.json` establish raw/control bounds, saved deadline and three-second polling. Store `ffmpeg_policy.rs:38,78` provides bounded ceil-to-poll delay and a weak per-call Store reader; media `ffmpeg/mod.rs:136` captures once, and `:174` kills/waits on timeout. `deadline_tests` (`:365,388`) assert a real child is killed/reaped and an in-flight call retains its captured deadline while later calls change; explicit fixed config wins. `tests/model/ffmpeg_timeout.rs:34,119` replay control/default/Cancel/concurrent updates and a real import-review timeout. `tests/gui/ffmpeg_timeout.rs` covers hidden/retired Options and already-open importer consumption; duplicates/content::timeout_tests adds real PSD consumption. About/import/folder/parser/sidecar paths attach the same reader in current source. Fixed/streaming APIs intentionally retain pre-existing deadlines. New authored process tests are Unix-gated, so they alone do not establish Windows process termination; platform CI remains central. No new omitted default-timeout reader path found in this bounded review.

## filesize-predicate

**Recommendation:** Retain conditional first-pass proposal for the exact size editor leaf.

**IDs:** `audit-options-predicate-filesize-size`

Python `ClientGUIPredicatesSingle.py:3148` and `ClientGUIBytes.py:9` supply five comparison choices, bounded amount and independent binary unit; `filesize_predicate.json` records bounds/reopening and actual query hashes. `tests/model/filesize_predicate.rs:45` compares operators/default/bounds/unit predicates and supplied-value precedence. `tests/gui/filesize_predicate.rs:48` uses real radio keys, all cases/units and actual Store query hashes, preserves unrelated defaults/reopens; `:195` tests hidden/cancelled/rebound/accepted-closed Main. Native `ui/filesize_comparison.slint` and predicate field kind8 wire focused radio selection; consumer `hydrus-search/src/exec/plan.rs` uses typed size/unit. Approximation/query rounding is asserted by recorded boundary hashes. No source defect found for claimed bounded amount/unit controls. Physical pointer/native font review remains central.

## gallery-source

**Recommendation:** Retain conditional first-pass proposal for the default-source control.

**IDs:** `audit-options-downloading-gallery-downloader-default-download-source`

Python actual `_default_gug._Edit`/GUG functionality is recorded in `gallery_source.json`. `tests/model/gallery_source.rs:51,90,142` compare key/name captions, installed parser/URL-class validity, category order/selection/Cancel/no-definition warning and cyclic-bound handling. `gallery_source_window.rs:63` owns the two-stage picker with active retirement and weak slot cleanup; Options acceptance stages a draft. `tests/gui/gallery_source.rs:111,191` exercise category child/parent Cancel, stale callback, saved/reopened default and gallery-page/subscription Add consumers. Native one-window categories differ from consecutive Qt dialogs, as disclosed; imported cycles are deliberately rejected. This packet has no count flag, so credit only its one exact ID after central checks. Category table rendering and the authored non-functional image still need central inspection.

## idle-time-maintenance

**Recommendation:** Demote all 12 first-pass/countAsCompletedLeaf claims to Partial / zero completion credit; retain status-activity Partial0.

**IDs:** `audit-options-maintenance-and-processing-duplicates-auto-resolution-idle-ideal-work-packet-time`, `audit-options-maintenance-and-processing-duplicates-auto-resolution-idle-rest-time-percentage`, `audit-options-maintenance-and-processing-duplicates-auto-resolution-work-duplicates-auto-resolution-in-idle-time`, `audit-options-maintenance-and-processing-file-maintenance-idle-throttle`, `audit-options-maintenance-and-processing-file-maintenance-run-file-maintenance-during-idle-time`, `audit-options-maintenance-and-processing-potential-duplicates-search-idle-ideal-work-packet-time`, `audit-options-maintenance-and-processing-potential-duplicates-search-idle-rest-time-percentage`, `audit-options-maintenance-and-processing-potential-duplicates-search-normal-ideal-work-packet-time`, `audit-options-maintenance-and-processing-potential-duplicates-search-normal-rest-time-percentage`, `audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-idle-consider-the-system-busy-if-cpu-usage-is-above`, `audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-idle-on`, `audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-idle-run-maintenance-jobs-when-the-client-is-idle-and-the-system-is-not-otherwise-busy`, `audit-options-status-activity`

Reference `MaintenanceAndProcessingPanel.py:143,167,217,228,296,427,446,526` supplies independent options and writes. `ClientController.py:2531` samples CPU > percent on at least count cores; as in reference v688, SystemBusy is status-only (caller `ClientGUI.py:5466`), so do not invent a missing worker CPU gate. Source does connect settings: CLI `main.rs:559,617,675` selects real similar-search, file-maintenance and auto-resolution work/rest/throttles from the marker; GUI `maintenance_runtime.rs:180` publishes idle and samples status CPU; `session_autosave.rs:309` determines user/mouse/API idleness. However this manifest cites only `idle_state.rs::tests`: `:88` fresh marker and `:103` Pace arithmetic. Neither test records actual Qt defaults/bounds/None options nor exercises native Apply/Cancel/reopen, independent setting import, real worker admission/budget/rest, live transition, failure/shutdown or CPU sample boundaries. This is an evidence gap rather than demonstrated broken consumer wiring. Linux-only /proc CPU sampling and two-minute boot delay difference are material (`DIFFERENCES.md:3491`). Add real reference control recording and meaningful worker/Options regressions before completion credit; existing global options fixtures by filename do not prove these twelve claims.

## local-service-followup

**Recommendation:** Retain conditional first-pass proposals for the two exact follow-up leaves.

**IDs:** `audit-media-services-missing-deleted`, `audit-media-services-missing-rating-preview`

`service_deleted.json` and `service_rating_preview.json`/`rating_preview_pointer.json` are actual Qt/DB examples. `tests/gui/services_review.rs:417,563` require both clear-record answers, preserve trash/history/import data on decline/close, read changed import consumer, reopen counts and reject retained/replaced owners; store `content/tests.rs::service_deleted_record_clear` verifies physical storage/deletion invariants. Rating samples `tests/model/services_editor.rs:503,650` compare real samples and pointer routes; `tests/gui/services_editor.rs:396,726,853` cover independent live examples, whole-widget drag/fraction hit area, staged child/parent Apply/Cancel, fresh samples and no file-rating persistence. Current source separates samples from ServiceKind and owns numeric prompts. The main Manage Ratings middle-click packet is a separate owner, and this evidence must not be transferred to it. Native chord capture cross-edge limits remain documented. No defect found in these bounded follow-up owners; central platform/render remains pending.

## manage-ratings-incdec-middle-click

**Recommendation:** Demote first-pass/countAsCompletedLeaf claim to Partial / zero completion credit.

**IDs:** `audit-media-ratings-missing-count`

Python `ClientGUIRatings.py:577` opens a modal edit-value spinbox0..1,000,000 and changes the rating only after Accepted. Native `manage_ratings_window.rs:173` opens an independent EditValueWindow; `:188` child Apply checks only weak upgrade, and `:203` Cancel merely hides. No active/visible/exact-child or parent-lifetime guard exists, no cleanup retires/hides child on Manage Ratings Cancel, and another middle click can replace the slot while old child remains actionable. Concrete path: open child A, Cancel A, then invoke A Apply; it still stages count into the live parent, which later parent Apply can persist. Closing parent while child is open also leaves the child displayed. Cited `tests/model/ratings_editor.rs:222` just directly calls count/set_count, checks a wrong kind and negative clamp; it neither records nor invokes a middle click/child Cancel/native persistence. Existing general ratings fixture lacks this action. UI `edit_value.slint:13` has correct maximum, but a matching spinbox alone is insufficient. Reuse the owned prompt pattern from service-rating preview; test real middle click, lower/upper bounds, accepted/cancelled child, replaced child, parent Cancel/X and persisted/reopened file ratings.

## media-focus

**Recommendation:** Retain conditional first-pass proposals for the two exact focus gates.

**IDs:** `audit-options-media-viewer-animation-audio-seek-bar-seek-bar-full-height-pop-in-requires-window-focus`, `audit-options-media-viewer-hovers-hover-windows-hover-window-pop-in-requires-window-focus`

Reference `MediaViewerPanel.py:59,103,150,224`, media container seek-height and real hover activation recording `viewer_focus_options.json` establish independent policies and forced-full exceptions. `viewer_focus.rs:31,65` routes the shared native application-focus registry by actual WindowId, keeping backend callbacks weak; `viewer_presentation.rs` reads live settings and Slint hover/seek visibility consumes it. `tests/model/options_dialog.rs::viewer_focus_controls_stage_independent_reference_policies` and store import regression prove staging/migration. Native `tests/gui/options_window.rs:2865` compares inactive/active/another/None focus combinations, real mouseover, held scrub/popup exceptions, Apply/Cancel/legacy and observer identity/owner release. Headless identity injection exercises the identical registry callback but does not replace final OS focus validation. Preview/duplicate-specific hovers remain unclaimed. No source defect found in stated slice.

## menu-choice-wheel

**Recommendation:** Retain Partial / zero completion credit.

**IDs:** `audit-options-gui-misc-mouse-wheel-can-scroll-through-menu-buttons`

Reference `ClientGUIMenuButton.py`/`GUIPanel.py` and actual `menu_choice_wheel.json` establish sign-based one-choice wrap, unsupported current value and disabled bubbling. Native model next() and shared Slint wheel overlay preserve real ComboBox pointer/key input; page/Manage Tags/manual export consumers are wired. `tests/gui/menu_choice_wheel.rs:91,225,283,444,592,736` compare real media order/results, focused native pointer selection, empty/single-value event protocol, staging/Cancel/retired root, actual tag rows/manual export and flat traversal. Model tests replay expected sign/wrap and source order. Full global MenuChoiceButton coverage is intentionally not claimed; packet stays Partial0. No new defect found in represented consumers. Physical wheel backend differences and final image inspection remain central.

## notebook-tree

**Recommendation:** Retain conditional first-pass proposal for the exact show-tree option.

**IDs:** `audit-options-gui-pages-navigation-and-drag-and-drop-experimental-show-tab-tree-view`

Actual `notebook_tree.json`/`tab_presentation.json` capture Qt cursor/expansion/activation and saved None/Left/Right. Native model `page_tree.rs:36` retains stable-key cursor/expansion separately from shown page; `tab_presentation.rs::bind_tree` routes disclosure/cursor/activation; main NotebookTree displays hierarchy. `tests/model/page_tree.rs:5,77` compare full recorded state and key changes. Native `tests/gui/notebook_tree.rs:28` dispatches real pointer and focused keys and asserts current page is unchanged by cursor/disclosure; `tests/gui/tab_presentation.rs:142` covers Apply/Cancel/reopen/side consumers. Double-click consumer callbacks are replayed, not a guarantee of platform physical double-click semantics. Missing drag/drop/cog/context/filter/history/depth controls remain outside this one exact option claim. Central native render/geometry pending.

## physical-delete-delay

**Recommendation:** Retain conditional first-pass proposal for the exact delay control.

**IDs:** `audit-options-files-and-trash-when-maintenance-physically-deletes-files-wait-this-long-between-each-delete`

Python `FilesAndTrashPanel.py`, `ClientFilesManager.py` and actual `physical_delete_delay.json` establish raw fields/minimum, captured pass wait and per-pair behavior. Native `physical_delete.rs:39,52` clamps display once on acceptance and preserves concurrent live policy; purge maintenance captures preference per pass, waits through PurgeControl and handles successful/missing/failing file/thumbnail pairs. `tests/model/physical_delete_delay.rs:26` replays constructor/raw/clamp/Cancel/save/reopen/concurrent behavior; native `tests/gui/physical_delete_delay.rs` exercises real Options hidden/retired Cancel and saved policy. Store maintenance tests check actual filesystem deletion ordering, missing paths, failures, reimports and interruptible waits; daemon `physical_deletes.rs:97` checks owner shutdown/releases Store. No defect found in bounded delay path. Long real physical deletion runs/platform filesystem behavior and exact render remain central.

## preview-viewing-intervals

**Recommendation:** Retain conditional first-pass proposals for the two exact preview time limits.

**IDs:** `audit-options-file-viewing-statistics-min-time-to-view-on-preview-viewer-to-count-as-a-view`, `audit-options-file-viewing-statistics-cap-any-view-on-the-preview-viewer-to-this-maximum-time`

Python `FileViewingStatisticsPanel.py:31,32,62,63,98,99` supplies None/time limits; actual preview Canvas recording is `preview_viewing_intervals.json`. Native `preview_window.rs:26` captures weak page incarnation, two bounded decoder workers and generation; only accepted owned frames start Tracker, with live saved finish policy in model viewing_statistics. Native `tests/gui/preview_viewing.rs:119,277,400,472,612,749,803` compare recorded page/splitter/close intervals, saved open-display cap/Cancel/confirmed exit, minimum/cap order and inactive finish, same-file restore/late decode, worker bounds, failure/no-placeholder charging and nonlocal/no-resolution/do-not-show refusal. Model `tests/model/viewing_statistics.rs:83,127` assert exact recorded Preview category/time conversion. The still/poster scope differs from Qt pre-decoder acceptance and lacks playback/hover/rating controls as stated; it is enough for these scoped time-limit consumers, not a preview family promotion. Pending central real render and runtime.

## related-tag-weights

**Recommendation:** Retain conditional first-pass proposal for the exact nested weights editor.

**IDs:** `audit-options-nested-tag-suggestions-weights`

Python `TagSuggestionsPanel.py:179,534` holds independent weight lists; `related_tag_weights.json`/`related_weight_table.json` capture actual questions/warnings/ranks/header sort. Native `related_weights_window.rs:28` owns active parent/child/question drafts; model Editor retains each table selection/sort and protects catch-alls; `related_tags_worker.rs` and store rank implement live consumer. `tests/model/related_weights.rs:6,73,114,182,221,347` compare namespace validation, independent cosine weights/ranks, real service mappings, unrelated preference merge, binary64 truncation and header behavior. Native `tests/gui/options_window.rs:5880` exercises child/question/parent Cancel, acceptance/save/reopen and re-ranking of an already-open service panel; `tests/gui/related_weight_table.rs:34` asserts sort/selection/retired callbacks. Full-corpus computation, local-service-only Manage Tags, missing query time budgets/sampling/exclusion and large top100 tie scope stay explicit. No source defect found in bounded recorded corpus. Central native/render pending.

## session-weight-report

**Recommendation:** Demote first-pass/countAsCompletedLeaf claim to Partial / zero completion credit.

**IDs:** `audit-options-weight-detail`

Python `ClientGUI.py:6960,8081` builds report from actual active and closed-page counts/seeds; native `pages.rs:3012,3062` walks live/closed snapshots and reads importer queues, and `menu_bar.rs:822` dispatches report. The strings and1/20 weights align by source inspection. Cited `tests/model/session_weight.rs:6` supplies synthetic (3,(1500,100),1,(10,2)) and asserts hardcoded string fragments only; it does not compare a Qt recording, actual nested notebook counts, multiple importer queue weights, closed/Undo snapshots, the visible menu command/report or post-change refresh. No arithmetic defect established here; the assessment is demoted for absent recorded/native consumer proof. Add actual frame GetTotalPageCounts/ShowPageWeightInfo recording plus native nested search/downloader/closed Undo test before credit.

## sibling-colours

**Recommendation:** Retain conditional first-pass proposals for the two exact sibling colour controls.

**IDs:** `audit-options-tag-presentation-other-rendering-fade-the-colour-of-the-sibling-connector-string-on-qt6`, `audit-options-tag-presentation-other-rendering-namespace-for-the-colour-of-the-sibling-connecting-string`

Python `TagPresentationPanel.py:71,75,133,134,306,332,334` defines independent fade/None namespace with disabled custom colour; `sibling_colours.json` records actual segmented Qt rows/paint extent. Core tag_presentation, model Manage Tags/write_autocomplete and native `tag_text.rs`/Slint preserve segments and previous solid trailing colour. `tests/model/sibling_colours.rs:83,191` compare staging/cancel/reopen, namespace fallback/count/ideal rows and collapsed-parent fade. Native `tests/gui/sibling_colours.rs:94` tests saved/retired Options and actual segmented Manage Tags/write/relationship consumers, unchanged drafts/selection and pixel blocks/trailing fixed extent under resize; store import regression preserves None/raw namespace separately. No broad tag-rendering family promotion. Pixel assertions are useful but native font/geometry/gradient appearance still require central image review; no defect found in this slice.

## subscription-exchange

**Recommendation:** Retain Partial / zero completion credit.

**IDs:** `subscriptions-exchange`

Actual `subscription_exchange.json`, legacy/cache recordings and `subscription_import_flow.json` capture permitted-type filtering, warnings/missing logs and ordered file accepted prefixes. Native codec `subscription_import.rs`, subscription_legacy/seed_cache and GUI list-owned import state preserve complete histories and owner decisions. Codec tests `tests/subscription_import.rs:6,85` compare nested/wrong-type and future/limits; native `tests/gui/subscriptions.rs:1213,1378,1458,1528,1622,1828,2040,2087` exercise staged export/import, clipboard/file/PNG choices, invalid/unreadable prefixes, actual notices/selection, missing history accept/refuse, parent/picker Cancel, retired notices and saved/reopened full caches. Type-filtered unrelated payload validation, clipboard bitmap precedence, drops, full native error bodies remain outside slice. Export compatibility remains partial in documented ways. No new defect found in inspected import ordering/owner path. No completion credit; native codec/test execution remains central pending.

## tag-autocomplete

**Recommendation:** Retain twelve conditional exact first-pass proposals and four existing Partial claims; no parent/alias promotion.

**IDs:** `audit-options-tag-editing-tag-edit-autocomplete-by-default-select-the-first-tag-result-with-actual-count-in-write-autocomplete`, `audit-options-tag-editing-tag-edit-autocomplete-when-pasting-multiline-content-into-a-write-autocomplete-skip-the-yes-no-check`, `audit-options-tag-editing-tag-edit-autocomplete-show-parent-info-by-default-on-edit-write-autocomplete-taglists`, `audit-options-tag-editing-tag-edit-autocomplete-show-parents-expanded-by-default-on-edit-write-autocomplete-taglists`, `audit-options-tag-editing-tag-edit-autocomplete-show-sibling-info-by-default-on-edit-write-autocomplete-taglists`, `audit-options-tag-editing-tag-edit-autocomplete-autocomplete-list-height`, `audit-media-tags-autocomplete`, `siblings-autocomplete`, `parents-autocomplete`, `audit-network-options-additional-tags`, `audit-options-tag-autocomplete-tabs-children-tags-how-many-tags-to-show-in-the-children-tab`, `audit-options-tag-autocomplete-tabs-favourite-tags-favourite-tag-list-editor`, `audit-options-search-or-create`, `audit-options-search-or-rewind`, `audit-options-search-or-cancel`, `audit-options-search-or-advanced`

Actual `write_tag_autocomplete.json` records Tags/Tag Editing settings plus write-list counted result selection, rendered sibling/parent rows, paste decisions, favourite/children rows; `read_or.json`/`read_or_editors.json` record real OR draft/editor behavior. Native write_tag_window/manage_tags/tag_relationships/import_options owners use detached draft children; `search_or_window.rs` and page consumer use captured OR ownership. `tests/model/write_autocomplete.rs` compares recorded row/selected/count/decorators/paste/favourite/children and settings; `tests/model/search_or.rs` compares draft/rewind/Cancel/advanced parse. Native `tests/gui/write_autocomplete.rs:8,111,184,259,308` exercises Manage Tags, relationship and import child, owner paste Cancel, stage/apply boundaries and live favourite/child cap. `tests/gui/options_window.rs:2744` covers favourite child/parent Apply/Cancel/reopen. `tests/gui/read_or.rs:44,182,421` covers real keys/current Store query, basic/advanced children, malformed input and stale/hidden/switched/locked owners. The six write-list preferences, child limit, favourite editor and four OR actions have scoped assertion support; larger Manage Tags/relationship/additional-tags roots stay Partial. Current later tests add menus/history and must not inflate this packet's credit. Pixel-height sizing differs from Qt character metrics; final native runtime/render remains pending.

## tag-sync-review

**Recommendation:** Demote first-pass/countAsCompletedLeaf claim to Partial / zero completion credit.

**IDs:** `audit-media-menu-tags-review-current-sibling-parent-sync`

Python `ClientGUITagDisplayMaintenanceReview.py:94,185,299` drives status, async per-service rules/backlog and work-now; native `tag_sync_review.rs:36,53` presents immediately synced graph rules, a documented architectural equivalent (`DIFFERENCES.md:3448`). `tests/model/tag_sync_review.rs:7,27` assert hardcoded status/zero rules/fresh Store; store `display.rs:408` directly asserts a handcrafted graph count5. There is no actual Qt fixture or native tab/default/remember/refresh/close test. Concrete owner defect: `tag_sync_review_window.rs:129` close uses global OPEN.take without captured identity. Retain closed A; open B; invoke A.close -> B is hidden/removed. A.service_chosen still writes default_service (`:103`) without active/visible/exact owner. Repair with owner-local active flag and weak exact current slot, guard callbacks, then record actual synced service rows/default tab and replay native remember/refresh/retired successor handling. The no-backlog/no-work-now native difference is accepted scope, not an invented missing native job.

## thumbnail-navigation

**Recommendation:** Retain three conditional exact first-pass proposals.

**IDs:** `audit-options-thumbnails-interaction-when-shift-selecting-move-the-navigate-from-here-position-with-it`, `audit-options-thumbnails-interaction-do-not-scroll-down-on-key-navigation-if-thumbnail-at-least-this-visible`, `audit-options-thumbnails-interaction-experimental-scroll-thumbnails-at-this-rate-per-scroll-tick`

Actual `thumbnail_navigation.json` captures Qt focus-vs-last-hit sequences, visibility threshold and wheel rate parsing/rounding. Native model selection::move_focus_with_last_hit and thumbnail_navigation::scroll_target/single_step feed live SearchPage and real Slint wheel/key callbacks; store ThumbnailNavigation imports independent keys. `tests/model/thumbnail_navigation.rs:15,71,105` compare staged controls/recorded origin/strict threshold neighbors. `tests/gui/thumbnail_navigation.rs:83` exercises saved/cancel/reopen, real pointer Shift/right-key selection/focus, actual Down reveal on each side of threshold and physical wheel without changing selection. Three-line/page-cap wheel policy, nonfinite/overflow saved text handling and missing platform fractional accumulation are disclosed. No preview focus or grid-family credit. No source defect found in represented regular vertical grid; final narrow/large geometry/native render pending centrally.

## viewer-prefetch

**Recommendation:** Retain three conditional exact first-pass proposals.

**IDs:** `audit-options-speed-and-memory-image-cache-maximum-of-cache-that-will-be-prefetched-per-media-viewer`, `audit-options-speed-and-memory-image-prefetch-num-previous-to-prefetch-in-media-viewer`, `audit-options-speed-and-memory-image-prefetch-num-next-to-prefetch-in-media-viewer`

Python `SpeedAndMemoryPanel.py:78,169,387,503`, Canvas neighbour order and real ImageRendererCache/DataCache behavior are recorded in `viewer_prefetch.json`. Native `viewer_prefetch.rs:39` captures supported raster candidates and saved policy; `:169` worker is one-latest-queue/per-owner, weak Store between passes/waits, with terminal generation retirement. Shared image_cache::prefetch_once enforces actual ready RGB/RGBA footprint, abort-on-pending/unknown/refusal, one miss and atomic finished-only flush. `tests/model/viewer_prefetch.rs:31,83,166,195` compare circular order, ready/pending/equality/unknown/atomic flush and concurrent normalized option merge. Unit worker tests in `src/viewer_prefetch.rs:387,439,468,493` cover coalescing, readiness, held-owner release and uncached equality repeats. Native `tests/gui/viewer_prefetch.rs:73,253,320` assert live Options/Cancel/retired changes reach actual viewer/archive/duplicate caches without changing current image or presentation intervals, and owner/cache teardown. Supported native static-image action scope excludes GPU/tile/video and Qt global shared async renderer topology; in-progress disk decode remains owned until it returns, current synchronous decoding may overlap. Failure placeholder differs explicitly. No defect found in this bounded review; central real image/cache execution and final viewer-prefetch-options PNG remain pending. No parent/cache-family credit.

## windows-headless-lifetime-support

**Recommendation:** Retain support-only repair; zero feature/parent/alias/completion credit.

**IDs:** None; support-only claims=[]

Current `headless.rs:18,31,49` registry holds weak adapters, final explicit collector hides components/releases adapters and is must_use; there is no TLS cleanup destructor. The removed fallback could run Store writer join under Windows TLS loader lock, but source risk is not proof of historical hung thread. `tests/gui/headless_lifetime.rs:7,53,95` preserve callback/Store release, cloned collector lifetime, explicit component release before thread return and bound menus/pages/rows/workers release with original two-second Store deadline. `.github/workflows/rust.yml:205,209` adds bounded five-minute Windows lifetime filter before default-parallel full suite. Current helper ownership was inspected: downloader replay uses windows inside its retained guard and search-lock receives caller collector. No Qt/render surface changed, so new recording/render credit is unnecessary. The historical 476 call count is source-provenance metadata, not proof of current platform execution; final source CI and Windows diagnosis remain central. Retain claims=[] and do not count this as a new leaf.
