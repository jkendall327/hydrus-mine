# Independent source review, batch 1

Reviewed `/workspace/hydrus-mine` at integration checkpoint `3702cdbee` by reading the assigned manifests, AGENTS.md, architecture/conventions/roadmap and area documentation, Python reference handlers and recordings, native implementation and authored assertions. No builds, test executions, mutation tests, network operations, repository edits or commits were performed. This report is source review, not executed validation. Exact-source CI and native rendering remain pending centrally. Historical manifest line numbers often differ from current integration lines; symbols below identify current code.

**Retain** means retain a scoped implementation proposal conditional on central CI/render results, never mark fully validated from this review. **Demote** means Partial and zero additional completed-leaf credit until the named defect/evidence boundary is closed. Support-only packets and existing Partial entries retain zero credit. Every assigned manifest is addressed; all IDs are listed in the appendix.

## Findings requiring action

1. **Auto-resolution silently reports failed work as successful.** `crates/hydrus-gui/src/auto_resolution_review_window.rs`, `State::decide`, catches `action_pairs` errors only by logging, then sets `done=true`. Its timer unconditionally runs `finish_decide` on every requested row. Since `crates/hydrus-gui-model/src/auto_resolution_review.rs::action_pairs` commits four-pair chunks, a failed chunk leaves unprocessed pending rows in the database but removes them from the displayed list. Carry a Result/committed identities and refetch on failure; assert an injected writer failure after a successful prefix.
2. **Auto-resolution's four-second popup is not independent of chunk work.** Native `action_pairs` tests elapsed time only before each chunk. A single long chunk never publishes its progress popup even after four seconds. Python `ClientGUIDuplicatesAutoResolutionRuleReview.py::_ApproveSelected` schedules `CallLater(4, pub, 'message', job_status)` before invoking `ActionAutoResolutionReviewPairs`. Native needs independent owned publication while work is blocked, plus finish/error cleanup assertions. The same applies to denial.
3. **Explicit shutdown-maintenance Cancel exits and registers a completed pass.** Python `hydrus/client/gui/ClientGUI.py:9342` uses `check_for_cancelled=True`, then `if was_cancelled: return`. Native `crates/hydrus-gui/src/client_exit.rs::shutdown_work` maps `on_cancelled` to `answer(false)`, which calls `shutdown_work::register` and `then()` to finish exit. Keep explicit Cancel/X separate from No/15-second auto-no. The maintenance timer is also deliberately leaked with `std::mem::forget`; replace it with an owned question lifetime and reject retired callbacks. The existing `client_exit` regression guards the earlier exit question, not this later maintenance child.
4. **Selected maintenance can operate on unrelated files.** `crates/hydrus-gui/src/thumbnail_maintenance_window.rs::run_now` enqueues the captured selected files, then calls `FileImporter::run_file_maintenance_of(n, ..., |j| j == job)`. `crates/hydrus-import/src/maintenance.rs::run_file_maintenance_inner` loads `file_maintenance::due_jobs_of`, whose SQL filters only job type and due time, with LIMIT 256. A backlog of the same type may consume all n slots before any selected file runs. This is especially material for destructive integrity jobs. Add captured-ID execution/selection filtering and a backlog regression.
5. **Retained thumbnail/database questions can accept after cancellation or replacement.** `thumbnail_maintenance_window.rs::ask` and `database_maintenance_window.rs::open` hide the question but never retire or validate it; a retained old `SessionDialog` sender can invoke affirmative handling again, including after Cancel. Their shared slots track a visible child only, without an answered/current identity gate. Add one-shot/current-owner guards and native stale/cancel regressions before granting complete workflow credit.

## Per-manifest findings

### active-predicate-completion-support.json

**Retain support-only Partial, zero credit.** The packet intentionally has `claims=[]`; supported ID is `audit-options-search-active-edit`. `predicate_editors/batch.rs::simple_predicate`, `Editor::mixed` and `mixed_predicates` stage and validate complete batches. `predicate_editor_window.rs` performs publication only after batch validation. Python `ClientGUISearch.py::EditPredicatesPanel` and `ClientGUIPredicatesSingle.py::GetPredicateFromSimpleTagText` establish the simple/system/invertible topology. `active_predicate_mixed.json` has 14 parser/veto cases and eight mixed dialogs; `active_predicate_or.json` has ten OR shapes. Model `active_predicates.rs::mixed_controls_replay_real_qt_values_row_order_cancel_and_parser_vetoes` actually decodes and compares recorded predicates and errors. Native tests `mixed_apply_is_atomic_and_hidden_cancel_rebind_preserve_all_original_terms`, `mixed_same_family_panels_reopen_distinct_supplied_values_and_apply_both`, and `populated_or_and_start_or_replay_all_ten_actual_qt_apply_cancel_shapes` assert the claimed boundaries. Hidden/source/rebind/weak-child tests verify owned transport. Mixed embedded OR, full inherited actions and selection/navigation remain outside this packet; newer inherited-menu tests must not retroactively give this support packet duplicate completion credit.

### auto-resolution-approval-progress.json

**Demote its single first-pass/count claim.** Successful four-pair batching is source-backed by Python `ClientDuplicatesAutoResolution.py::ActionAutoResolutionReviewPairs`. The manifest-cited model test `approving_and_denying_report_progress_as_the_reference_s` checks three authored strings; it does not replay timing, popup admission or errors from a recording. `auto_resolution_review.json` records review rows/actions, not the new four-second boundary. Native review tests wait for successful completion. Both defects 1 and 2 above contradict the actual progress workflow. Disable/working state is first set by a 100ms timer after worker start, rather than synchronously at admission, so source also leaves a brief re-entry period before presentation catches up (State guards prevent a second work item, but other callbacks are not uniformly gated). No cancellation requirement is invented: the reference job itself is noncancellable.

### content-undo.json

**Demote both additional completed-leaf proposals to Partial on evidence scope.** `hydrus-store/src/undo.rs` records archive/inbox and supported mapping inversions; `Store::write_undoable`, `undo` and `redo` apply through real content transactions. `manage_tags.rs` records a Package after accepted changes, and `menu_bar.rs` invokes store undo/redo then `reshow`. Python `ClientManagers.py::UndoManager` is the relevant specification. However, the claimed reference recording of archive/inbox and Manage Tags undo does not exist in the inspected oracle tree. `tests/model/content_undo.rs::archiving_can_be_undone_and_redone_from_the_menu` checks Store and menu Facts directly with authored strings, not a native menu; `tag_changes_name_their_service` writes a direct Package and checks its name, without undoing/redoing an actual ManageTags apply. `undo.rs` tests log truncation only. Add a real reference replay and native tag/inbox refresh assertions. Store advances the log index before a fallible write; Python does likewise, so this is a missing failure boundary rather than a claimed parity divergence. Only the stated archive/inbox/Manage Tags operation subset may earn credit eventually.

### database-maintenance-cacheless.json

**Recommend demote eight original repair/regeneration leaves to Partial; preserve the honest navigation/no-op slice.** `database_maintenance.rs::run` explicitly performs no maintenance for LocalTags, SimilarTree, PendingCount, ServiceInfo and ResyncDeleted; LocalHashes emits success, RepopulateMappings reports zero recovery, and OrphanSerialisables reports no orphans. The native architecture legitimately lacks those Python cache tables. `record_database_maintenance.py`/`database_maintenance.json` and model tests replay real questions/buttons/service choices and clean-client popup strings, which supports a menu compatibility slice. It does not prove native damaged-state repair or actual native question cancellation: the “refusing asks nothing more and writes nothing” assertion examines the recorded Python event only. There are no dedicated native database-maintenance tests in the inspected GUI test tree, and defect 5 affects its current question callbacks. Original labels describe repair/regeneration, so copying a clean-client success alone should not become eight fully implemented repair leaves. If the project expressly counts architecture-obsolete repair controls as first-pass no-op navigation, retain only after its inventory scope explicitly says this and native cancel/current-owner assertions exist; never infer cache corruption recovery.

### debug-fetch-url.json

**Retain one finite first-pass implementation proposal.** Python `ClientGUI.py` debug URL handler and `record_debug_fetch_url.py` execute actual NetworkJob/engine behavior against an authored loopback transport. `debug_fetch_url.json` records binary bytes, decoded clipboard text, response question, save and real dismissal. Native `debug_fetch.rs::State` assigns stable IDs, weak callback ownership and terminal retirement; `start` uses `NetEngine::fetch(Request::get(...))` and saves bandwidth accounting, while popup stop cancels the actual Job. Tests `actual_menu_binary_save_decoded_clipboard_forget_cookie_headers_and_dismiss_deadline`, `overlapping_held_jobs_popup_stop_error_and_cancelled_save_have_real_consumers`, `hidden_children_close_cancel_rebind_final_bound_drop_and_main_drop_retire_owned_work`, and `ordinary_get_waits_for_saved_bandwidth_rules_and_popup_stop_cancels_before_any_http` assert concrete consumers, bytes and adverse paths. This supports the declared GET action; it does not validate every network engine or platform file-picker behavior. Source `pick_debug_response` returns a path synchronously and checks binding lifetime again before writing. Native rendering and execution remain pending.

### duplicate-filter-prefetch-control.json

**Demote one count proposal to Partial pending consumer evidence.** Python SpeedAndMemoryPanel has the 0–25 control, default loaded through options, and computes `2 + num_pairs * 2` for its warning. Native `filter_window.rs` reads `Preferences.duplicate_pairs` and `duplicate_filter.rs::upcoming` includes the current pair plus N later pairs; code is connected to the owned real viewer-prefetch cache. However, neither `tests/model/options_dialog.rs` nor inspected native GUI tests contain an assertion targeting the duplicate-pairs option. `viewer_prefetch.rs` tests vary previous/next/percentage; its fixture retains `duplicate_pairs` as an unchanged sibling and checks merge preservation, not edited duplicate-filter neighborhood/cache requests. Add recorded pair-control bounds/Apply/Cancel and a real duplicate filter consumer assertion for zero, nonzero, last batch and edited policy. Shared viewer-prefetch evidence alone is not the required distinct consumer boundary.

### external-calls.json

**Retain five bounded first-pass proposals and all fifteen Partial entries at zero credit.** `ExternalProgramsPanel.py` and `external_calls.json`/`external_command.json` record default factories/platform choices, sort ties, selected insertion order, partial duplicate warning prefixes, OS-default call deletion, parameter reordering and literal clipboard parsing. Native `options_external_calls.rs`, `external_call_window.rs`, model `external_calls.rs` and `external_command.rs` preserve stable call keys and staged descendant drafts. Native tests `duplicate_warning_decline_keeps_unsorted_unselected_prefix_accept_finishes_and_retired_question_cannot_append`, `simple_delete_includes_os_launch_rows_but_cancel_and_closed_owner_do_not_change_saved_calls`, and `list_duplicate_defaults_delete_capture_and_options_persistence` assert the three table leaves against recorded questions and selected rows. Parameter/clipboard tests actually replay queue events, dispatch keyboard events, persist cleaned vectors and assert stale owner refusal; model tests cover reverse-selected Edit, anchor histories, warning repr and all size boundaries. `reopened_saved_process_uses_real_editor_inputs_and_owned_test_call_worker` covers a real harmless process, while model cancellation reaps its direct child. Keep broad Add/Edit/import/export/actual/process/PATH/timeout/output parents Partial: legacy executable-manager migration, complete UI/OS launch breadth, descendant process groups and exact output/error presentation are explicitly omitted. Scope is registered typed calls; the separate Open Externally packet owns production routing credit.

### files-view-removal.json

**Retain four finite option/consumer proposals.** Python FilesAndTrashPanel, Canvas filter commit and `ClientMediaList.py` pruning underpin `files_view_removal.json`'s 16 settings, 12 filter outcomes and 120 content-event cases. Model `actual_qt_media_list_trash_move_and_physical_pruning_matrix` executes store transfers/deletes and compares removal membership; `filter_removal_matches_recorded_commit_gates_and_skipped_return_file` covers filter flags and selection return. Native `file_view_removal.rs` tests actual Options Cancel/reopen/retired callbacks, filter Accept/Forget/Resume/page switch, one- versus multiple-domain trash behavior, already-trash/locked no-op, strict transfer conflict and rebound/forgotten source owners. The implementation consumes actual committed/transferred IDs and post-write membership rather than guessing from a requested action. Remaining broad query refresh/foreign API behavior and all deletion-dialog families are separate. Four settings are independent controls; do not count supporting callbacks as additional leaves.

### frame-locations.json

**Retain four bounded table action proposals; retain three wider entries Partial.** `GUIPanel.py` frame list and `EditFrameLocationPanel` generate `frame_locations.json`; native `frame_locations.rs`, `options_frames.rs` and `windows.rs` stage flip/reset by stable row identities. Model `frame_table_defaults_cells_and_selected_actions_match_actual_qt` compares recorded cell/selection state; native `frame_options_table_child_staging_and_saved_viewer_geometry_match_reference` replays all four operations, Cancel, child ownership, save/reopen and actual media viewer size/state/reset consumption. Its final loop consumes `options_geometry_lifecycle.json` and verifies Options owner saved geometry independently. Position persistence is asserted, but MinimalSoftwareWindow lacks a native position adapter: do not claim an executed OS coordinate placement test. Default gravity/mouse/parent positioning, screen fit and broader frame editors remain Partial. The unsupported desktop placement boundary is disclosed rather than promoted.

### idle-scheduling-consumers.json

**Demote its three count proposals pending behavioral consumer replay.** Native `idle_state.rs` publishes a timestamped marker atomically and rejects stale/future/invalid markers. CLI `main.rs` similar search, file maintenance and auto-resolution loops do read current idle state and choose the appropriate active/idle gates; file maintenance uses throttle budgets rather than Pace. `idle_state` tests assert marker freshness and a small pure Pace selection/rest example. The manifest cites source only and contains no reference recording/native regression for transitions actually admitting or preventing each of these three workers. This supports Partial improvement over unconditional active scheduling, but callback/read presence and a generic pacing unit test cannot prove the three original scheduling workflows. Separate normal-time-maintenance evidence is for trash/deferred-delete gates and must not be reused as evidence for similar search/auto-resolution/file-job scheduling. CPU/global idle and wake scheduling differences remain Partial.

### import-work-slots.json

**Retain five finite staged capacity proposals.** `record_import_work_slots.py` records real ImportingPanel 1–500 controls and controller acquire/release across all five categories; `import_work_slots.json` captures raw invalid-load normalization and lower/grow behavior. `work_slots.rs::Slots` holds independent counts and RAII Permit release; QueueRunner maps gallery files/searches, watcher files/checks and other workers to those categories and rereads settings while pending. Native `actual_importing_controls_cancel_apply_reopen_and_ignore_retired_edits` compares recorded values, actual owner staging/stale callbacks and reopened Store. Backend tests replay five permit traces and test owner abort releasing only its category; URL queue and watcher tests hold real work through capacity lowering/growth/error/cancel/close. Queue allocation and owner-qualified seed update regressions protect retired identities. Cancellation during blocking local import may finish its current work and FIFO fairness/cadence are deliberately omitted. Keep those limits; no new subscription credit belongs here.

### main-window-identity.json

**Retain Partial/zero for both the claimed activation option and the existing application-name refinement.** Python GUIPanel, ListBoxes `_NewSearchPages` and GUI focus code provide the reference. `main_window_identity.json` includes raw names, four activation combinations and actual middle press. Native `viewer_tag_search.rs`, `main_identity.rs` and owned launcher in `lib.rs` use canonical tags, current binding/source file and captured search context. Native tests assert real middle pointer production, page/location/default consumers and stale file/hidden/closed/rebind refusal; the raw-name test checks Cancel/default normalization and captured main-title refresh. Source requests native focus only when inactive. Wayland focus support, OS actual activation/rendering and Qt global applicationDisplayName propagation remain missing: Partial is correct, and title refinement must not be counted again.

### media-cursor.json

**Retain one finite cursor option proposal.** `viewer_cursor_options.json` records strict timeout boundaries, polling cadence, actual menu and motion states. Model `viewer_cursor.rs::CursorWait` matches `elapsed > delay`, 100–250ms polling and None; native `viewer_cursor.rs::NativeCursor` owns a SingleShot timer, per-Winit WindowId weak motion registration, actual popup stack queries and OS `set_cursor_visible`. `options_window.rs::cursor_timeout_reaches_native_motion_timer_focus_and_actual_popup_lifecycle` checks staged Cancel/reopen, motion/focus/menu and close; `session_autosave` pointer tests assert ID isolation and weak registration cleanup. Keep backend MPV mouse/hover boundaries separate. Native OS cursor visibility cannot be inferred merely from headless property state; central platform/render review remains required.

### media-zoom-loop.json

**Retain two bounded proposals.** `viewer_zoom_loop_options.json` supplies 16 zoom traces, six loop traces, animation metadata and bytes. `zoom.rs::switch_with_policy` and native configurable-hover test compare actual geometry/position; Options GUI test checks Apply/Cancel/reopen and owner policy consumption. Native `animation.rs` reads live always-loop policy at playthrough, while metadata parsing establishes finite/zero loop counts and existing MPV uses its player policy. Unconditional decoded-frame test `animation_loop_preference_reaches_decoded_frames_and_live_playthrough_limits` checks actual stop/loop behavior; MPV regression is conditional on libmpv availability. The loop leaf does not add missing native GIF/APNG decoders. This is first-pass scope over existing playback backends, not universal animated MIME playback validation. Hover command policy is captured at viewer construction as disclosed.

### normal-time-maintenance.json

**Retain two finite admission-gate proposals.** Python `ClientDaemons.py`/`ClientFilesManager.py` and `normal_time_maintenance.json` establish independent default-true gates, entry-time admission, admitted setting changes and shutdown between groups. `maintenance_gates.rs::load` correctly gives native settings precedence over retained ClientOptions and `save_changed` preserves the untouched peer; load errors propagate from runtime rather than silently admitting. `maintenance_runtime.rs` owns off-thread jobs and cooperative retirement/wakes. Native tests exercise actual disposable file removal: unchecked normal flags block both workers, live GUI idle admits both, a waiting pass consumes Apply and retirement/rebind/last binding drop wakes the held physical wait without targeting successors. Existing broad CPU/global-idle/wake/cadence, batching and IO boundaries remain Partial. Do not expand the two control credits to entire trash/deferred maintenance parents.

### parser-children.json

**Retain the scoped first-pass entries as authored, with existing parser-test/formula-test excluded from new completion; retain sidecar-router and manual-export examples Partial.** Python `ClientGUIParsing.py` implements subsidiary creation/editing, one-choice source timestamp and raw-test child contexts; metadata migration/sidecar GUI handlers establish source/router example starting strings and JSON names. The packet's individual recordings (`parser_children`, `subsidiary_exchange`, `sidecar_testing`, `router_exchange`, `parser_raw_preview`, `export_folder_examples`, `export_pattern_shortcuts`, `sidecar_json_names`, `content_time`) are specific to these behaviors rather than generic evidence filenames. Native parser and sidecar tests decode tuple payloads, reject wrong type/direction, stage child Apply/Cancel, retire exchange descendants, execute real router/manual-export consumers and compare sidecar content. `raw_content_preview_preserves_clipboard_context_and_detects_fetched_png_bytes` uses actual loopback PNG data and verifies context, MIME gates and stale fetch callbacks; model raw preview asserts Unicode clipping lengths and recorded JSON/HTML strings. Timestamp model/native tests normalize the sole type to 0, and `seeds.rs::recorded_saved_timestamp_parser_and_date_conversion_reach_actual_file_seeds` executes saved parsing into source-time seeds, including None/nonnumeric/future clamping. Source/queue mixed unsupported payloads reject atomically instead of reference valid-prefix acceptance; this is explicitly documented. Keep export-each-PNGs, unknown processors/nonstub timestamp routing, broad tag maintenance and full keyboard navigation outside credit. This large packet combines twelve entries: two already-existing first-pass test entries and two Partial parents must not become extra original completions.

### preview-default-zoom.json

**Retain one finite preview zoom proposal.** `preview_default_zoom.json` has six control choices and 48 ordinary plus 48 DPR cases from real Qt preview geometry; Python MediaPlaybackPanel and `ClientGUICanvasMedia.py` establish independent preview-specific policy and no geometry refresh merely on save. `preview_zoom.rs::rect` applies preview scale rules, integer/DPR centering and clipping; `preview_window.rs` reads the policy on accepted raster or viewport/DPR changes, preserving current geometry on ICC/cache replacement. Model tests compare every recorded rect, preserve full-viewer policy, Cancel, legacy fallback/native wins and concurrent untouched fields. Native tests cover actual six modes/paint clipping, held raster acceptance, hidden/current owner and prohibited admission. Keep backend/media-family, other zoom menu and cache policies outside this single control. Central render review must verify measured viewport geometry rather than only source Slint bindings.

### regex-options-editor.json

**Retain first-pass supported editor status with zero additional leaf credit.** The packet explicitly records that the original leaf already had completion credit. `record_regex_options_editor.py`/`regex_options_editor.json` drive actual Add/Edit/Delete plus saved-favourites RegexInput copy menus. Native test `real_options_saved_input_chooser_crud_cancel_apply_and_retired_owners_match_qt` replays rows/errors/copy, rejects empty description, blocks parent page/search/sibling children reciprocally, checks hidden parent refusal and old callback retirement, then verifies persisted favourites in real StringMatch consumer. Model tests compare original sorted/deletion/advisory semantics and native-wins empty values. The distinction between read-only saved chooser and staged draft is asserted. Broad regex validity/engine coverage is separate; this improves an already completed bounded editor and must not increment the completion ledger.

### service-bulk.json

**Retain two bounded first-pass proposals.** `service_bulk.json` records 22 real trash/rating confirmation cases. Native `services_review_window.rs` captures service/action in Pending and rejects hidden/retired changes; model routes accepted work through ContentWriter. `local_bulk_review_replays_confirmations_store_changes_and_reopens` compares exact questions/statistics against the fixture, dispatches Escape, blocks service changes while asking, checks persistence, correct rating scopes and other services; backend content tests cover physical deleted/nonlocal distinctions, trash membership and archived-file lock. ClearTrash uses normal transactional deletion/deferred physical queue; Undelete restores previous local domains/import times. Ratings scope uses physical-storage presence so trash is still local. No broad remote service maintenance equivalence is inferred from these local bulk controls. File bytes/deferred processing platform execution remains central.

### shutdown-maintenance.json

**Demote the three shutdown controls and force-maintenance workflow; treat Restart as a source-supported proposal requiring native process evidence.** `shutdown_work.rs` reproduces due-period strictness, action labels, analyze work and max-minute stop; `tests/model/shutdown_work.rs` asserts authored decision strings and actual table-analysis registration. It is not a reference recording and does not test real Options staging, maintenance-child Cancel or native process relaunch. The reference's explicit cancellation branch and native divergence are defect 3 above. `client_exit` keeps only its earlier exit question lifetime safe; maintenance child timer/answers are not scoped to binding retirement. `main.rs` sets/spawns the restart command after event loop exit, but inspected tests do not exercise a real relaunch or child failure boundary. Keep supported analyze-only shutdown work documented; repository/file/other shutdown task breadth remains outside the native implementation. A separate patch proposal for explicit Cancel/timer ownership is being prepared for the root to apply.

### subscription-concurrency.json

**Retain one finite concurrency option proposal.** Python SubscriptionsManager ready/finished scheduling is recorded with scripted membership/due times, honestly not a reference HTTP concurrency run. `subscription_concurrency.json` has 11 cases and a 120-second completed-run buffer. Native SubscriptionRunner rereads committed limits, holds one job per persistent ID, prevents admissions on pauses/shutdown and exposes active IDs. Native Options test (current symbol `subscription_concurrency_options_replay_bounds_parent_apply_cancel_and_reopen`, moved from historical line 3301) compares clamping/Apply/Cancel/reopen; backend tests replay scheduling cases and hold actual loopback HTTP requests to assert capacity growth, lowering without cancellation, global pause, exactly one request per ID, targeted cancellation and shutdown join. Open editor manager-pause flag remains missing, invalid imported out-of-range handling is defensive and no new multi-job monitor UI is claimed. These disclosed differences do not invalidate the finite capacity control, but do constrain parent parity.

### tab-presentation.json

**Retain three finite presentation proposals, conditional on render inspection.** Python PagesNotebook/tab painting and `tab_presentation.json` record four alignments, six hide gates and six elision cases. Native `tab_presentation.rs` and `notebook_tabs.slint` use stable PageKey paths, all selected-depth bars, real glyph measurements, selected font weight, horizontal/vertical rotation and overflow reserved space. Tests `apply_cancel_reopen_and_four_sides_preserve_real_nested_selection_and_full_names`, `recorded_hide_gate_retains_live_hierarchy_access_and_elision_changes_actual_paint`, and `selected_glyph_measurement_triggers_elision_at_the_actual_near_fit_boundary` assert actual geometry, interior rendered ink, pointer hierarchy navigation, full stored names/tooltips and scroll-to-last exposure. This goes beyond a flag-presence test. Tree selector is explicitly a limited navigation fallback, and complete Qt style/drag semantics remain separate. Central inspection of narrow/vertical output is still necessary.

### tag-suggestions.json

**Retain width, column layout and most-used favourites proposals; retain default-page Partial.** Python TagSuggestionsPanel and SuggestedTagsPanel generate eight layout/default-page cases with related/lookup disabled. Native `tag_suggestions.slint` uses saved width in minimum-size/layout consumers; ManageTags captures layout at construction and updates lists through the owned timer. Model replay compares available-page fallback, per-service merges and add-only filtering; GUI Options tests stage/cancel nested write-tag children and retain unrelated service edits. `manage_tags.rs::most_used_panels_filter_only_add_broadcast_and_retire_closed_consumers` asserts actual suggestions, saved width/layout, add-only staged mappings, broadcast refresh, Apply and retained closed refusal. Related/File Lookup availability remains outside this packet's ordinary default-page parity even if later work adds a related panel; do not promote this packet's Partial leaf or reuse its disabled-related fixture for the later consumer. Current geometry is expressed as a minimum width, not an exact fixed desktop measurement.

### thumbnail-maintenance.json

**Demote maintenance and selected viewing-stats workflow proposals pending consumer/owner regressions.** `dump_regen_jobs.py`/`regen_jobs.json` proves all 27 labels/descriptions/order. Model tests compare fixture wording and author zero/one/large question expectations; they do not execute native questions or workers. Maintenance has defect 4; both maintenance and stats share defect 5's retained question acceptance. `viewing_maintenance::clear_files` is a real selected-ID database consumer, but no native test here asserts Cancel, stale callback, selected-only row changes or reopened stats. Add backlog/captured-ID worker tests and one-shot questions, then actual clear-stats acceptance/Cancel/other-files retention. Full underlying job implementation is separately covered by importer maintenance fixtures; that does not prove this thumbnail entrypoint captures and executes the correct selection.

### viewer-eye-menu.json

**Retain three finite collapse-option proposals, with a native menu materialization caveat.** `viewer_eye_menu.json` records all eight preference combinations and window actions from the actual Qt menu; native `viewer_eye_menu.rs` reads current saved settings on each opening and `viewer_eye_menu.slint` conditionally groups/flattens real supported rows with recorded separator boundary logic. Model option test compares all combinations; native `eye_menu_collapse_options_stage_reopen_and_rebuild_the_existing_browser_viewer` asserts detached edits do not affect saved/live viewer policy, stale Apply, reopen and existing-viewer consumption. `eye_menu_mixed_root_boundaries_match_the_recorded_menu_and_real_declaration_order` checks grouped row data and declaration order; the manifest admits it does not introspect the generated native menu tree. Thus it is strong source evidence for finite settings/row grouping, but exact native popup order/pointer behavior still needs central rendering. Do not count missing ICC/duplicate-filter deeper actions or broader eye/hover families.

### window-rescue.json

**Retain all three entries Partial/zero.** `window_rescue.json` records five staged controls and 47 topology/ordinary cases; model `safe_position` matches lenient top-left, Qt three-corner order, independent rescue fuzz and fallback. Native OpeningRescue delays consumption until visible nonzero size, applies once and composes Winit opening/drop filters in `watch_named_events`; model/native tests replay recorded topology and actual consumer decisions. Source documents that Winit exposes full monitor geometry, not Qt available work-area origin, and global mixed-DPI/compositor placement plus unkeyed/default positioning remain incomplete. Its shared-filter regression addresses lost drop handlers and native inner-size timing rather than pretending headless tests prove OS positioning. Existing Partial/count-zero assessment is accurate; no self-sizing exception or whole window rescue credit.


## Reviewed ID appendix

All statuses here are implementation proposals; none are fully validated.

### active-predicate-completion-support.json

- `audit-options-search-active-edit` — support-only; Partial/0.

### auto-resolution-approval-progress.json

- `audit-media-review-progress` — manifest proposes first_pass. See manifest-specific recommendation above.

### content-undo.json

- `audit-options-undo-manager-undo-last-content-operation` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-undo-manager-redo-last-content-operation` — manifest proposes first_pass. See manifest-specific recommendation above.

### database-maintenance-cacheless.json

- `audit-media-menu-database-check-and-repair-repopulate-truncated-mappings-tables` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-media-menu-database-check-and-repair-resync-combined-deleted-files` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-media-menu-database-clear-orphan-hashed-serialisables` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-media-menu-database-regenerate-local-hashes-cache` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-media-menu-database-regenerate-local-tags-cache` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-media-menu-database-regenerate-service-info-numbers` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-media-menu-database-regenerate-similar-files-search-tree` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-media-menu-database-regenerate-total-pending-count-in-the-pending-menu` — manifest proposes first_pass. See manifest-specific recommendation above.

### debug-fetch-url.json

- `audit-options-help-debug-action-fetch-a-url` — manifest proposes first_pass. See manifest-specific recommendation above.

### duplicate-filter-prefetch-control.json

- `audit-options-speed-and-memory-image-prefetch-num-pairs-to-prefetch-in-duplicate-filter` — manifest proposes first_pass. See manifest-specific recommendation above.

### external-calls.json

- `audit-options-external-programs-external-calls-delete` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-external-programs-external-calls-duplicate` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-external-programs-external-calls-add-defaults` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-external-programs-external-calls` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-external-programs-external-calls-editor-list` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-external-programs-external-calls-add` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-external-programs-external-calls-edit` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-external-programs-external-calls-export` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-external-programs-external-calls-import` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-nested-external-call` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-nested-external-call-actual` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-nested-external-call-process` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-nested-external-call-command` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-nested-external-call-command-path` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-nested-external-call-command-arguments` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-nested-external-call-command-copy` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-nested-external-call-input-rules` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-nested-external-call-process-timeout` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-nested-external-call-test-availability` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-nested-external-call-test-execution` — manifest proposes partial. See manifest-specific recommendation above.

### files-view-removal.json

- `audit-options-files-and-trash-remove-files-from-view-when-they-are-archive-delete-filtered` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-files-and-trash-even-skipped-files` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-files-and-trash-remove-files-from-view-when-they-are-sent-to-the-trash` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-files-and-trash-remove-files-from-view-when-they-are-moved-to-another-local-file-domain` — manifest proposes first_pass. See manifest-specific recommendation above.

### frame-locations.json

- `audit-options-gui-frame-locations-flip-remember-size` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-gui-frame-locations-flip-remember-position` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-gui-frame-locations-reset-last-size` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-gui-frame-locations-reset-last-position` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-gui-frame-locations-editor-list` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-gui-frame-locations-edit` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-gui-frame-locations` — manifest proposes partial. See manifest-specific recommendation above.

### idle-scheduling-consumers.json

- `audit-media-menu-database-work-file-jobs-during-idle-time` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-media-preparation-scheduling` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-media-rule-sidebar-scheduling` — manifest proposes first_pass. See manifest-specific recommendation above.

### import-work-slots.json

- `audit-options-importing-work-slots-number-of-gallery-downloader-file-queues-that-can-import-at-the-same-time` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-importing-work-slots-number-of-gallery-downloader-searches-that-can-run-at-the-same-time` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-importing-work-slots-number-of-other-paged-importer-jobs-that-can-run-at-the-same-time` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-importing-work-slots-number-of-watcher-page-checkers-that-can-run-at-the-same-time` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-importing-work-slots-number-of-watcher-page-file-queues-that-can-run-at-the-same-time` — manifest proposes first_pass. See manifest-specific recommendation above.

### main-window-identity.json

- `audit-options-gui-main-window-switch-to-main-window-when-creating-new-file-search-page-from-media-viewer` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-gui-main-window-application-display-name` — manifest proposes partial. See manifest-specific recommendation above.

### media-cursor.json

- `audit-options-media-viewer-mouse-behaviour-time-until-mouse-cursor-autohides-on-media-viewer` — manifest proposes first_pass. See manifest-specific recommendation above.

### media-zoom-loop.json

- `audit-options-media-viewer-hovers-top-hover-button-menu-controls-zoom-switch-button-switches-between` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-media-playback-video-animations-always-loop-animations` — manifest proposes first_pass. See manifest-specific recommendation above.

### normal-time-maintenance.json

- `audit-options-files-and-trash-allow-trash-maintenance-during-normal-time` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-files-and-trash-allow-deferred-file-deletes-during-normal-time` — manifest proposes first_pass. See manifest-specific recommendation above.

### parser-children.json

- `subsidiary-create` — manifest proposes first_pass. See manifest-specific recommendation above.
- `subsidiary` — manifest proposes first_pass. See manifest-specific recommendation above.
- `sidecar-test` — manifest proposes first_pass. See manifest-specific recommendation above.
- `sidecar-source-processing` — manifest proposes first_pass. See manifest-specific recommendation above.
- `sidecar-router-processing` — manifest proposes first_pass. See manifest-specific recommendation above.
- `sidecar-router` — manifest proposes partial. See manifest-specific recommendation above.
- `parser-test` — manifest proposes first_pass. See manifest-specific recommendation above.
- `formula-test` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-network-export-folder-examples` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-network-export-examples` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-shared-sidecar-destination-json` — manifest proposes first_pass. See manifest-specific recommendation above.
- `content-time` — manifest proposes first_pass. See manifest-specific recommendation above.

### preview-default-zoom.json

- `audit-options-media-playback-zoom-and-position-preview-viewer-default-zoom` — manifest proposes first_pass. See manifest-specific recommendation above.

### regex-options-editor.json

- `audit-options-regex-favourites-regular-expression-favourites-editor` — manifest proposes first_pass. See manifest-specific recommendation above.

### service-bulk.json

- `audit-media-services-missing-trash` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-media-services-missing-ratings` — manifest proposes first_pass. See manifest-specific recommendation above.

### shutdown-maintenance.json

- `audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-shutdown-run-jobs-on-shutdown` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-shutdown-only-run-shutdown-jobs-once-per` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-shutdown-max-number-of-minutes-to-run-shutdown-jobs` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-menu-menu-file-exit-force-maintenance` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-menu-menu-file-restart` — manifest proposes first_pass. See manifest-specific recommendation above.

### subscription-concurrency.json

- `audit-options-downloading-subscriptions-maximum-number-of-subscriptions-that-can-sync-simultaneously` — manifest proposes first_pass. See manifest-specific recommendation above.

### tab-presentation.json

- `audit-options-gui-pages-navigation-and-drag-and-drop-notebook-tab-alignment` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-gui-pages-navigation-and-drag-and-drop-experimental-hide-main-page-navigation-tabs` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-gui-pages-page-tab-names-when-there-are-too-many-tabs-to-fit-elide-their-names-so-they-fit` — manifest proposes first_pass. See manifest-specific recommendation above.

### tag-suggestions.json

- `audit-options-tag-suggestions-suggested-tags-width-of-suggested-tags-columns` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-tag-suggestions-suggested-tags-column-layout` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-tag-suggestions-suggested-tags-default-notebook-page` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-nested-tag-suggestions-favourites` — manifest proposes first_pass. See manifest-specific recommendation above.

### thumbnail-maintenance.json

- `audit-media-context-missing-maintenance` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-media-context-missing-stats` — manifest proposes first_pass. See manifest-specific recommendation above.

### viewer-eye-menu.json

- `audit-options-media-viewer-hovers-top-hover-button-menu-controls-collapse-window-submenu-in-view-options-eye-menu` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-media-viewer-hovers-top-hover-button-menu-controls-collapse-hovers-submenu-in-view-options-eye-menu` — manifest proposes first_pass. See manifest-specific recommendation above.
- `audit-options-media-viewer-hovers-top-hover-button-menu-controls-collapse-rendering-submenu-in-view-options-eye-menu` — manifest proposes first_pass. See manifest-specific recommendation above.

### window-rescue.json

- `audit-options-gui-frame-locations-bugfix-disable-off-screen-window-rescue` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-gui-frame-locations-when-rescuing-add-top-left-safety-padding` — manifest proposes partial. See manifest-specific recommendation above.
- `audit-options-gui-frame-locations-debug-top-left-padding-to-use-px` — manifest proposes partial. See manifest-specific recommendation above.

