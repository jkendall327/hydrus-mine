# Batch 3 manifest review at 3702cdbee

Read-only review of all 26 assigned manifests. Read AGENTS.md, architecture/conventions/roadmap and relevant GUI/differences/oracle documentation. No build, tests, mutation runs, source edits or git changes were performed. “Retain” means the scoped proposal has plausible reference/source/assertion support. Historical milestone validation is distinguished below; new proposals are not fully validated. Exact-source CI, native execution, platform runs and rendered review are centrally pending. Source line citations describe the reviewed checkpoint; root repairs may change these afterwards.

## active-predicate-routes-support.json

IDs:

- `audit-options-search-active-edit`

**Recommendation:** Retain support-only; active-edit stays Partial, completion credit 0.

`active_predicates/routes.rs::copy_text/searches/menu` and GUI `active_predicates.rs:504–565` consume frozen selections, check owner/query identity and then use the clipboard/page launcher. `hydrus-gui-model/tests/model/active_predicates.rs:216` compares all 14 recorded menus, exact clipboard payloads, AND/OR/per-term predicate sets, page names/topics and no-op select-files publications against `active_predicate_routes.json`; native tests at `tests/gui/active_predicates.rs:821,960` cover the real transport, query consumers and hidden/modal/switched/closed/rebound guards. Python `ClientGUIListBoxes.py::_ProcessMenuCopyEvent/_NewSearchPages/_NewDuplicateFilterPage` and recorder `record_active_predicate_routes.py` support this inherited slice. The disclosed Qt set-order versus native list-order difference is retained. Do not convert support evidence into a second active-edit completion.

## autocomplete-tab-panes.json

IDs:

- `audit-options-search-tag-tabs-children`
- `audit-options-search-tag-tabs-favourites`

**Recommendation:** Retain both scoped first-pass proposals (2), conditional on central execution/render review.

`autocomplete.rs:354` builds favourites/descendants from stored settings, the current display graph and real display counts; `autocomplete_tabs.rs:13` routes selected batches and immediate favourite changes, and `watch:225` defers revision consumption while locked/noted/hidden. `tests/gui/autocomplete_tabs.rs:54` compares actual rows/selection/predicates/settings against `autocomplete_tab_selection.json`, covers OR draft cancellation, concurrent favourite writes and child retirement; `:493` proves literal wildcard/system-looking strings stay Tag predicates and empty activation preserves query/history. `tests/gui/read_autocomplete.rs::real_page_tabs_apply_caps` covers zero/finite/unlimited caps. Python `ClientGUIACDropdown.py` and `record_read_tag_tabs.py` record the real pane/search behavior, including cleaned descendant lookup versus literal activation. Narrow context-menu scope (single logical tag) is explicit; no full autocomplete-family promotion.

## database-backup.json

IDs:

- `audit-media-database-backup-path`
- `audit-media-database-backup-update`
- `audit-media-menu-database-restore-from-a-database-backup`

**Recommendation:** Demote all three to Partial / countAsCompletedLeaf=false until missing owner/failure/reference-backed assertions and restore defects are resolved.

Reference: `ClientGUI.py:906` saves Last/Exit sessions before synchronous backup, `:6838` performs path questions/checks; `ClientController.py:1963` restores after confirmation; `ClientDB.py:310–378` performs cancellable copy/mirror and unconditionally finishes its popup. Native `database_backup_window.rs:52–249` wires the flow, while `backup.rs:116–192` does actual SQLite copy/mirror/restore. Cited model tests `tests/model/database_backup.rs` check menu labels and only substring question text plus simple directory classification. Store `backup.rs::a_backup_restores_over_a_changed_store` verifies one happy-path setting and `a_mirror_copies_changes_and_drops_surplus` verifies mirror basics. Neither asserts picker/No/Cancel/retired owner, actual saved session, cancellation/failure outcome, or restart integration; there is no recorded backup oracle replay.

Concrete defects at reviewed source: `backup.rs:77–78` returns success on cancelled media mirroring and `:133–139` skips media then returns success; `database_backup_window.rs:205–221` marks completion and advances time. Python also reports completion/time after cancelled backup, so this is a native integrity/success-status concern and any correction should be documented as intentional. The native SQLite `run_to_completion` does not poll cancellation. More serious restore defects: `main.rs:27–31` performs restore before the GUI lock at `:37`; `backup.rs:172–187` deletes target DB before source copy, including the source itself when `from==dir`; `:157–166` removes the request before restore succeeds. No same/aliased-path check or preservation-on-copy-error assertion exists. Setup also checks only exact `path==store.dir()`, not a media-source overlap. Pending central/root repairs are not evidence for this reviewed checkpoint.

## database-password-and-bones.json

IDs:

- `audit-media-database-clear-password`
- `audit-media-menu-database-how-boned-am-i`

**Recommendation:** Demote both broad first-pass proposals to Partial / no completion credit; retain the recorded model slices.

Password: Python `ClientGUI.py:6737` and `record_set_password.py` record six cancel/keep/clear/match/mismatch/second-cancel cases with persisted reference hashes. `tests/model/set_password.rs:9` replays those questions and resulting LockPassword values, but saves into a local variable. `set_password_window.rs` writes actual settings, yet Cancel only hides its slot-owned SessionDialog; callbacks have no retired-owner guard. After Cancel/close a retained callback can call `answered(true)` and save a cleared password (`SetPassword::answered` is not restricted to a clear-question state). No native SetPassword/Store-reopen test covers this path; `tests/gui/unlock.rs` only checks a supplied lock during unlock.

How boned: `record_how_boned.py` records default and my-files domains, and `tests/model/how_boned.rs::the_basic_client_s_statistics_read_as_the_reference_s` exactly asserts table rows, earliest time, views, duplicates and Bones text for those two domains. `how_boned_window.rs` loads asynchronously and has a generation counter for Stop/close, but no native test asserts typed predicates, invalid input, Stop, stale delivery or reopening. Recorder/model tests contain no predicates. Thus the claim's additional typed-search/async GUI behavior is not established. No password-obscuring defect is asserted: the Python EnterText call also leaves password_entry at its default false.

## debug-session-reload.json

IDs:

- `audit-options-help-debug-action-close-and-reload-current-gui-session`

**Recommendation:** Retain scoped first-pass proposal (1), conditional on central execution/render review.

Python `ClientGUI.py:5733` really saves, force-closes, reloads and deletes its temporary named session. Native `debug_session_reload.rs:129` admits only active visible non-question owners, captures before spawning a real Store worker, delivers admitted work while hidden, and retires weak timer/page/Main callbacks permanently; `session_reload::take` removes its private snapshot slot. `tests/gui/debug_session_reload.rs:159` asserts recorded tree/media order, empty selection, hidden delivery, immutable admitted snapshot, distinct identities, Undo retention and no temporary slots; `:234` asserts fresh importer queue/log identities and paused old owners; `:304` covers Cancel/accepted exit, rebind, final shared clone and Main destruction. The recorder’s single real Qt media scenario does not prove every native importer edge; those are authored regression extensions. Worker/database errors are source-handled but not fault-injected here.

## duplicates-preparation-maintenance.json

IDs:

- `audit-media-preparation-numbers`
- `audit-media-preparation-tree`
- `audit-media-preparation-storage-resync`

**Recommendation:** Demote all three to Partial / no completed-leaf credit.

Python `ClientGUISidebarDuplicates.py:534–595` has actual regeneration and resync questions/commands. Native `duplicates_sidebar.rs:249–253` stages the questions, but accepted RegenerateTree/RegenerateNumbers deliberately do nothing at `:382`; there is no persistent tree/count cache to regenerate. This is a documented architecture difference, not implementation of the represented regeneration work. The sole cited assertion `tests/model/duplicates_filtering.rs:82` directly calls `similar::resync_potentials_to_local_storage`, proves two synthetic orphan groups lose pairs/search status, checks idempotence and handcrafted popup text. It does not replay a reference recording or assert menu/question acceptance/No, valid-pair preservation, retired ownership, popup delivery or failure. Resync is substantive backend work (`similar.rs:382`), but its broader native completion boundary is unsupported.

## file-exit-completion.json

IDs:

- `audit-options-menu-menu-file-exit`

**Recommendation:** Demote “whole flow” first-pass to Partial / no completion credit.

`client_exit.rs:136–203` provides initial confirmation and 15-second auto-yes, then `shutdown_work:36–104` supplies run/skip/ask and 15-second auto-no; restart is set after finishing. `tests/model/shutdown_work.rs:6` checks handcrafted decide strings/modes and `:47` runs analysis and reads last_done. The separate inline `client_exit.rs:213` regression checks old close/timer callback retirement for the initial question only. None asserts the full actual exit→maintenance-question→auto-no/No/Cancel→registered timestamp→restart chain, error behavior or main rebind during the second question. This cannot substantiate the manifest’s “whole flow” wording. Native-only maintenance-kind limitation remains correct; no new recorded shutdown flow is cited.

## folder-run-menu-regressions.json

IDs:

- `audit-options-menu-menu-file-check-all`
- `audit-options-menu-menu-file-check-import-folder-now-folder`
- `audit-options-menu-menu-file-run-all`
- `audit-options-menu-menu-file-run-export-folder-now-folder`

**Recommendation:** Retain the narrowly stated flag-dispatch improvement; do not call the four menu leaves fully validated. Recommend Partial/0 until a reference-backed menu dispatch replay exists.

Python `_CheckImportFolder` at `ClientGUI.py:1032` and `_RunExportFolder:6139` flag selected/all named settings, warn when globally paused and publish a wake notification. Native `folder_runs.rs` writes exactly the selected/all check_now/run_now fields and `menu_bar.rs:624,629` calls it. `tests/model/folder_runs.rs::named_then_all_folders_are_flagged_to_run` asserts only direct helper calls and booleans, not menu dispatch, paused warnings, error paths or notification/wakeup. `tests/model/main_menu.rs:240` checks offered labels; worker tests `hydrus-download/tests/{import_folder,export_folder,folder_activity}.rs` consume these flags independently. This split supports backend dispatch, but there is no reference fixture assertion for these four new promotions. No cancellation question is expected for these actions; absence of a Cancel test is not itself a defect.

## gui-coloursets.json

IDs:

- `audit-options-colours-override-what-is-set-in-the-stylesheet-with-the-colours-on-this-page`
- `audit-options-colours-current-colourset`
- `audit-options-menu-menu-help-darkmode`

**Recommendation:** Retain the three scoped first-pass proposals; structural colour-picker family stays Partial / 0.

Python `panels/options/ColoursPanel.py:124–125` separates override from current colourset; `ClientGUI.py::FlipDarkmode` warns then flips. `gui_colours::Settings::save_changed/load` preserves unedited roles/current-set changes and legacy fallback; `gui_colours.rs:17` uses weak owner-local observers, `gui_colour_actions.rs:43` gates/retire pending acknowledgements. `tests/model/gui_colours.rs:54,123,154` compare all 26 RGB roles and staged/concurrent/import behavior. `tests/gui/gui_colours.rs:88,201` assert picker Cancel/stale/hidden/Help acknowledgment/final Bound drop; `:308` proves a palette change discards stale fade brushes without redecoding; `:455` tests local/trash/remote thumbnail membership and semantic RGB paint across owned consumers against `gui_coloursets.json`. Native RGB-only picker topology is correctly excluded from concrete credit. Render tests remain authored, not inspected/executed by this reviewer.

## idle-timeout-options.json

IDs:

- `audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-idle-permit-idle-mode-if-no-general-browsing-activity-has-occurred-in-the-past`
- `audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-idle-permit-idle-mode-if-your-mouse-cursor-has-not-been-moved-in-the-past`
- `audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-idle-permit-idle-mode-if-no-client-api-requests-in-the-past`

**Recommendation:** Retain browsing/API scoped first-pass proposals (2); retain mouse Partial/0.

`session_autosave::Monitor::idle_at/poll_at` consumes loaded thresholds and independent activity timestamps; native event observer routes application-window input, API uses its timestamp-only marker. `tests/model/idle_timeout_options.rs` replays `idle_timeout_options.json` and constructor normalization recording for defaults, None, floor/clamp, unchanged Apply versus Cancel and concurrent implicit normalization. `tests/gui/idle_timeout_options.rs:50` asserts an actual saved live autosave gate, exact strict threshold, independent browsing/mouse/API actions, and durable reopen; `:177` asserts unchanged raw-second normalization affects the existing gate. Python real MaintenanceAndProcessingPanel/CurrentlyIdle recorder holds boot/activity clocks. Global OS mouse movement remains unsupported as correctly disclosed. A working autosave consumer supports these specific timeout controls, not every high-CPU/idle worker family.

## local-transfer-confirmations.json

IDs:

- `audit-options-files-and-trash-confirm-when-copying-files-across-local-file-domains`
- `audit-options-files-and-trash-confirm-when-moving-files-across-local-file-domains`

**Recommendation:** Retain both scoped first-pass proposals (2), conditional on central execution/render review.

`Transfer` captures source/destination stable keys and ordered file identities; `local_transfer_window.rs` checks parent guard, active/visible question owner and closes stale questions. Store `content/local_transfer.rs` implements copy/strict move/merge, preserving destination timestamps on restored deleted membership. `tests/gui/local_transfer.rs:138` compares staged/default/Cancel/save/reopen and concurrent owner settings to `local_transfer_confirmations.json`; `:197` asserts actual yes/no/cancel writes and stale-window behavior; `:331` triggers actual thumbnail menus, frozen selection and A→B→A identity retirement. Python recorder observes real modal migration worker writes and refreshed DB media. Failure displays owned error and leaves the question live; injected Store failure is not independently exercised here. Scope is local membership, not physical relocation or remote services.

## manage-tags-counts-incremental.json

Historical status: 2 listed IDs already occur in the pinned overnight validated milestone (`docs/rust/gui-coverage/overnight/progress.json`), so retain that historical status; current-source integration/render verification remains pending. 

IDs:

- `audit-media-tags-missing-deleted`
- `audit-media-tags-missing-incremental`

**Recommendation:** Retain both scoped first-pass proposals (2), conditional on central execution/render review.

`ManageTags::deleted_tags/deleted_count/current_tags` loads per-file deleted/current mappings; global show_deleted changes are immediate while tag changes remain private draft. `tests/gui/manage_tag_counts.rs:37` compares actual rows/counts with `manage_tag_counts_incremental.json`, other open-owner refresh, outer Cancel and retired callbacks. `IncrementalTagging::pairs/cleaned_pairs` freezes original file order and service; owned child callbacks block parent edits/Apply. `tests/gui/incremental_tagging.rs:16` asserts inferred defaults, reversed/negative/zero-step assignments, invalid input without draft mutation, live text preference retention across Cancel, per-file persistence, successor retirement and one-file refusal. `tests/model/incremental_tagging.rs::actual_qt_initial_clamps_and_long_decimal_previews_preserve_safe_native_inference` covers Unicode/overflow boundaries. The documented native 1024-character preview safety limit remains an explicit difference; it does not invalidate the ordinary number-assignment slice.

## media-hovers.json

Historical status: 4 listed IDs already occur in the pinned overnight validated milestone (`docs/rust/gui-coverage/overnight/progress.json`), so retain that historical status; current-source integration/render verification remains pending. 

IDs:

- `audit-options-media-viewer-hovers-hover-windows-pop-in-tags-left-hover-window-on-mouseover`
- `audit-options-media-viewer-hovers-hover-windows-pop-in-ratings-and-locations-top-right-hover-window-on-mouseover`
- `audit-options-media-viewer-hovers-hover-windows-pop-in-notes-right-hover-window-on-mouseover`
- `audit-options-media-viewer-hovers-background-draw-index-text-bottom-right-in-the-viewer-background`

**Recommendation:** Retain four scoped first-pass proposals, conditional on central execution/render review.

`ViewerHoverSettings` imports inverted disable_* keys; `viewer_presentation::refresh` supplies independent tags/ratings/notes hover enables and passive index text, consumed by `viewer.slint` existing hit regions and background stacking. `tests/model/options_dialog.rs::viewer_hover_controls_replay_reference_enabled_states` and import assertions compare enabled/default/interlock semantics. `tests/gui/options_window.rs::hover_options_apply_to_actual_mouseover_panels_and_passive_index_text` uses the actual current viewer/Options Apply/Cancel and independent mouseover states against `viewer_hover_options.json`, including passive string/inset. The real Python recorder exercises ideal layouts and background/index drawing. Earlier independent viewers, preview/duplicate hover controls and complete focus/layout topology are unclaimed; no whole-hover promotion.

## metadata-file-jobs.json

IDs:

- `audit-media-times-disk`
- `audit-media-force-rename`

**Recommendation:** Retain both Partial improvements and completion credit 0.

`metadata_jobs.rs:245` uses file/64-file block cancellation boundaries, per-hash claims, copy-only imported-media ownership, >3-second popup publication and delayed neighbour cleanup; accepted worker lifetime belongs to `metadata_file_jobs::Jobs`, independent of the editor. `hydrus-store/tests/metadata_jobs.rs:127` compares actual reference worker inputs, publication/progress arrays, file existence, bytes and ms timestamps, including partial cancellation and rename-copy fallback, to `metadata_file_jobs.json`. Native Manage Times/Force Filetype tests assert editor Cancel/stale owned callbacks and accepted work. The recording replaces DB command transport, so it does not prove actual Python DB writes; cleanup eligibility is asserted but runner/retry lifecycle, filesystem lock/recovery and global shutdown/error parity remain outside scope. These explicit limits justify retaining Partial.

## notes-preferences.json

IDs:

- `audit-options-notes-start-editing-notes-with-the-text-cursor-at-the-end-of-the-document`
- `audit-options-notes-when-middle-clicking-a-note-hover-only-copy-the-text`
- `audit-media-notes-missing-cog`

**Recommendation:** Retain three scoped first-pass proposals, conditional on central execution/render review.

Python real NotesPanel and Manage Notes cog/key/middle-click recording `notes_preferences.json` captures persisted global preferences, UTF-16 cursor positions, copy variants and dirty-note Cancel behavior. Native `manage_notes_window.rs:38` translates initial cursor state and remembers per-tab selections; `:549` reads/edits latest global preferences, active/visible child guard; `notes_editor::copy_with` preserves copy semantics. `tests/gui/notes_preferences.rs:97` translates Qt offsets to UTF-8 byte offsets, inserts a real Unicode key in the native editor, checks cog mouse routes, future-note behavior, six clipboard variants, actual hover middle click, global preference retention after owner Cancel, and no file-note mutation/reopened cursors. `tests/model/notes_preferences.rs:20` asserts exact reference copy variants. Viewer left-click/right-click note edit/hide and broader Notes family stay Partial as disclosed.

## popup-freeze-minimized.json

IDs:

- `audit-options-popup-notifications-popup-window-toaster-freeze-the-popup-toaster-when-the-main-gui-is-minimised`

**Recommendation:** Retain Partial / countAsCompletedLeaf=false, completion credit 0.

`popup_freeze.rs::minimized` preserves Winit Option<bool>, using actual Slint minimized state only for software-window fallback; `popups.rs:329` stops UI reconciliation while frozen without stopping Store producers/expiry. `tests/model/popup_freeze.rs:28,71` compare genuine Qt hidden/minimized/unfocused states and staged/concurrent preference changes; `tests/gui/popup_freeze.rs:79,205,272` assert queued/expired jobs, thaw, live policy, child Cancel/stale/rebind/accepted close. `record_popup_freeze.py` executes real manager add/update handlers while supplying clocks/private scheduler. Wayland reports unknown and has no faithful external minimized signal; other-monitor cursor freezing remains absent. Headless set_minimized is not OS event attestation. These are correctly conservative partial limits.

## quick-export-directory.json

IDs:

- `audit-options-menu-menu-file-quick-export-directory`

**Recommendation:** Retain scoped first-pass proposal (1), conditional on central execution/render review.

`quick_export_directory.rs::Control::open` rereads saved path, resolves only absent-path fallback, preserves explicit empty home, creates only fallback directory, normalizes missing portable paths, checks owner after resolver and before launch, and reports failures to a finished popup. `tests/gui/quick_export_directory.rs:71` compares `quick_export_directory.json` menu-launch paths, fallback creation/conflicting-file/undetermined errors, Cancel/Apply/reopen; `:252` exercises hidden/rebound/closed/reentrant resolver owners. Unit `explicit_empty_home_wins_and_account_lookup_only_runs_for_absent_home` distinguishes missing from empty HOME. Real Qt recorder is Linux; native normalized targets and existing OS launcher remain separate platform execution/render gates.

## rule-preview-pair-menu.json

IDs:

- `audit-media-preview-context`

**Recommendation:** Demote to Partial / countAsCompletedLeaf=false until a preview-consumer regression is authored/executed centrally.

Native `auto_resolution_preview_window.rs::clicked/show_selected` has selections for passing/failing lists, derives selected files and calls show_location/open_files; `auto_resolution_rules.slint` exposes actual context menu callbacks. Python `ThumbnailPairList._ShowSelectedPairsInNewPage` supplies the intended behavior. However the cited `tests/model/auto_resolution_review.rs` only asserts review-actions tab rows/text, action progress and labels; it never calls preview clicked/show_selected or a new-page launcher. The manifest itself admits no new recording/native GUI test. It therefore does not establish multi-selection, empty/wrong row, deleted-file location or owner-close behavior for this different preview surface. Callback presence is insufficient evidence.

## sessions.json

Historical status: 32 listed IDs already occur in the pinned overnight validated milestone (`docs/rust/gui-coverage/overnight/progress.json`), so retain that historical status; current-source integration/render verification remains pending. Only those milestone IDs retain historical validation; structural parents and existing Partial entries do not acquire it. 

IDs:

- `audit-options-tabs-context-action-2270-saved-session-name`
- `audit-options-tabs-context-action-2287-non-reserved-session-name`
- `audit-options-tabs-context-action-2290-create-a-new-session`
- `audit-options-tabs-context-action-2052-page-weight-information-advanced`
- `audit-options-tabs-context-action-2249-refresh-this-page`
- `audit-options-tabs-context-action-2253-refresh-all-this-page-s-pages`
- `audit-options-tabs-context-action-2164-new-page`
- `audit-options-tabs-context-action-2168-new-page-here`
- `audit-options-gui-pages-opening-and-closing-put-new-page-tabs-on`
- `audit-options-gui-sessions-sessions-if-last-session-above-autosave-it-how-often-minutes`
- `audit-options-gui-sessions-sessions-if-last-session-above-only-autosave-during-idle-time`
- `audit-options-gui-sessions-sessions-default-session-on-startup`
- `audit-options-gui-sessions-sessions-show-warning-popup-if-session-size-exceeds-10-000-000`
- `audit-options-import-options-help-for-this-panel`
- `audit-options-import-options-keep-this-panel-simple`
- `audit-options-import-options-default-import-options-editor-list`
- `audit-options-import-options-default-import-options-show-stack`
- `audit-options-import-options-default-import-options-edit`
- `audit-options-import-options-default-import-options-clear`
- `audit-options-import-options-default-import-options-reset-to-defaults`
- `audit-options-import-options-url-class-import-options-editor-list`
- `audit-options-import-options-url-class-import-options-show-stack`
- `audit-options-import-options-url-class-import-options-edit`
- `audit-options-import-options-url-class-import-options-clear`
- `audit-options-import-options-favourites-profiles-editor-list`
- `audit-options-import-options-favourites-profiles-add`
- `audit-options-import-options-favourites-profiles-edit`
- `audit-options-import-options-favourites-profiles-delete`
- `audit-options-gui-pages-opening-and-closing-in-new-page-chooser-show-combined-local-file-domains-if-appropriate`
- `audit-options-gui-pages-opening-and-closing-put-it-at-the-top`
- `audit-options-gui-pages-opening-and-closing-in-new-page-chooser-show-hydrus-local-file-storage`
- `audit-options-gui-pages-opening-and-closing-put-it-at-the-top-2`
- `audit-options-gui-pages-opening-and-closing-confirm-when-closing-any-page`
- `audit-options-gui-pages-navigation-and-drag-and-drop-maximum-entries-to-show-in-page-navigation-history`
- `audit-options-gui-pages-navigation-and-drag-and-drop-when-switching-to-pages-move-keyboard-focus-to-any-text-input-field`
- `audit-options-file-sort-collect-file-sort-namespace-file-sorting-ordered-editor-list`
- `audit-options-file-sort-collect-file-sort-namespace-file-sorting-add`
- `audit-options-file-sort-collect-file-sort-namespace-file-sorting-edit`
- `audit-options-nested-sort-collect-cog-default-file-sort`
- `audit-options-nested-sort-collect-cog-secondary-file-sort-when-primary-gives-two-equal-values`

**Recommendation:** Retain scoped behavior proposals with existing Partial limits; remove completion credit from three parent dialog rows and correct the inconsistent count header.

The 40 claims contain 35 first_pass/countAsCompletedLeaf=true, 3 first_pass/false and 2 partial/false, while completedOriginalLeafCount is 12. At frozen `4356918f` as well as current inventory, default-import-options-editor-list, url-class-import-options-editor-list and favourites-profiles-editor-list are dialog parents with 4/3/3 children. Their status can remain scoped first-pass; their leaf credit must be false. This leaves at most 32 literal credited leaves before central prior-ledger/dedup reconciliation.

Behavior evidence by subgroup: notebook save/append (`tests/gui/notebook_sessions.rs:75,244`, `notebook_sessions.json`) asserts exact questions, invalid names/Cancel, target notebook, Store reopen and independent media/queue snapshots. Refresh/weights (`notebook_refresh.rs:68,158`, `tab_refresh.json`) asserts recursive initialized descendants, locked/background page preservation and exact copied weights. New/Here/insertion (`notebook_new_page.rs:70,149,232`, `tab_new_page.json`) asserts real chooser order/location, frozen parent/anchor, Cancel and actual downstream new pages. Autosave/startup/warning (`session_autosave.rs:51,146,208,268,324`; `session_startup.rs:81,124,303,398,458`) asserts real archives/hash suppression, idle retry, API marker, startup recovery/frozen choice/clean marker and once-per-boot size popup against the corresponding reference fixtures. `session_autosave.rs` observes genuine application input, but global external mouse remains absent, so only-idle stays Partial/0.

Import Options (`tests/gui/import_options_panel.rs:73,267`, `import_options_panel.json`) asserts exact reset/clear/stack questions, invalid targets, shared child draft isolation, URL key/profile identity, Cancel and actual caller resolution after parent Apply; profile-delete stays Partial because the native intended single-name question differs from recorded Qt behavior. Page chooser/navigation (`page_chooser_options.rs:48`, `page_navigation_options.rs:59,135,241`) assert exact offered locations, disabled-value persistence, close/Undo/session exceptions, capped History and actual search/importer input focus. Namespace schemes/sort cogs (`namespace_sorts.rs:76,221`, `sort_cog.rs:62`, `sort_cogs.json`) assert staged queue/questions, duplicate identity, escaped parsing, Cancel, real sort/collect consumers and independent context keys; existing ordered-editor parent/cog credit=false is correct.

Source consumers in `pages.rs::{fresh_session_pages,append_session_to_notebook,append_session_backup,new_page_at,refresh_key,close_question}`, `session_autosave::Monitor`, `session_startup::Run`, shared import Options resolver and `sort_cog::choose` substantiate those scoped assertions. Claims here are not full session/history migration or drag parity. Native frozen snapshots use owned fresh queue/page identities; historical legacy backups are unclaimed. Runtime and native rendering remain centrally pending.

## sibling-connector.json

Historical status: 1 listed IDs already occur in the pinned overnight validated milestone (`docs/rust/gui-coverage/overnight/progress.json`), so retain that historical status; current-source integration/render verification remains pending. 

IDs:

- `audit-options-tag-presentation-other-rendering-sibling-connecting-string`

**Recommendation:** Retain scoped first-pass proposal (1), conditional on central execution/render review.

`TagPresentation::sibling_connector`, import decoding and Options exact text row flow into `ManageTags` storage row decoration and `WriteAutocomplete` suggestion labels. `tests/model/sibling_connector.rs:18` and `tests/gui/sibling_connector.rs:62` replay `sibling_connector.json` default/custom/empty/Unicode strings, stage/Cancel/save/reopen and real storage/write rows while asserting no staged mappings. Already-open consumers reread on refresh. Qt fading/separate connector namespace colours and whole-tag-presentation topology remain correctly excluded.

## subscription-failure-limit.json

Historical status: 1 listed IDs already occur in the pinned overnight validated milestone (`docs/rust/gui-coverage/overnight/progress.json`), so retain that historical status; current-source integration/render verification remains pending. 

IDs:

- `audit-options-downloading-subscriptions-if-a-subscription-has-this-many-failed-file-imports-stop-and-continue-later`

**Recommendation:** Retain scoped first-pass proposal (1), conditional on central execution/render review.

Python `ClientImportSubscriptions.py:1251–1292` excludes typed DataMissing, increments escaped errors, sleeps five seconds and raises exact abandonment text. Native `subscriptions.rs:934–1009` generates per-file options/writes query tags, preserves handled WorkOnURL statuses, saves the seed before testing the cumulative report threshold, and excludes typed DataMissing. `hydrus-download/tests/subscriptions.rs:852` proves real HTTP500/404/missing handled failures do not spend the budget and matches recorded status/error codes; `:921` triggers actual Store query-tag failure across queries, asserts saved seeds/pending remainder/configured delay and next-run reset; `:1025` proves None disables abandonment. Model/native Options and import assertions cover exact label/default/min/max, staging, Cancel/reopen and legacy None/numeric values. Low-level typed error exclusion is asserted separately; real filesystem/native Options fault injection is not claimed.

## tag-banner-options.json

Historical status: 6 listed IDs already occur in the pinned overnight validated milestone (`docs/rust/gui-coverage/overnight/progress.json`), so retain that historical status; current-source integration/render verification remains pending. 

IDs:

- `audit-options-tag-presentation-tag-banners-on-thumbnail-top`
- `audit-options-tag-presentation-tag-banners-on-thumbnail-bottom-right`
- `audit-options-tag-presentation-tag-banners-on-media-viewer-top`
- `audit-options-nested-tag-banner-appearance`
- `audit-options-nested-tag-banner-namespaces`
- `audit-options-nested-tag-banner-preview`

**Recommendation:** Retain six scoped first-pass proposals, conditional on central execution/render review.

`tag_banner::Editor` preserves ordered stable rows, frozen delete identities and live example cleaning; `tag_banner_window` provides three allow-blank prompts with Cancel and owned child Apply. Options stages each independent generator and parent Apply reaches thumbnail cache invalidation/RGBA plus the already-open viewer title through SingleMedia combined current/pending summary consumers. `tests/model/tag_banner.rs:11` exactly compares 40 Qt generator values/previews, namespace edits/cancellations and delete questions; `:105` asserts child/parent isolation; `:149` asserts Unicode decimal sorting/collapse against `tag_banner_sort_boundaries.json`. Native `tests/gui/options_window.rs::banner_options_match_qt_drafts_and_refresh_cached_thumbnails_and_open_viewer` covers populated old rows, all three generators, RGBA drawing, child/parent Cancel/stale slots and cached/open consumer refresh. Full generator/Options parents remain excluded; actual rendered RGBA/font placement awaits central inspection.

## tags-sync-menu.json

IDs:

- `audit-media-menu-tags-sync-now`
- `audit-media-menu-tags-sync-tag-display-during-idle-time`
- `audit-media-menu-tags-sync-tag-display-during-normal-time`

**Recommendation:** Demote idle and normal-time switches to Partial / no credit; sync-now can retain only an architecture-specific no-work slice, but recommend Partial/0 until a real popup dispatch assertion exists.

Python `ClientGUI.py:4036–4055` routes switches to real TagDisplayMaintenanceManager policy and `_SyncTagDisplayMaintenanceNow:7122` calls SyncFasterNow off Qt; `ClientTagsHandling.py:468,475` consumes both switches. Native `menu_bar.rs:810–837` flips saved BackgroundWork fields and unconditionally publishes all-synced text. Search of consumer settings and manifest limits confirms switches have no runtime consumer; synchronous native graph application is the documented reason (`DIFFERENCES.md:880`). The cited `tests/model/main_menu.rs` checks menu/check-state generation, not actual dispatch, durable toggling or popup Store publication. No queued sync work exists natively, so the no-work message is sensible architecture adaptation, but it does not prove the policy controls or generic sync-now work. No cancellation is expected for simple toggles.

## thumbnail-preview-selection.json

IDs:

- `audit-options-thumbnails-interaction-on-ctrl-selection-focus-thumbnails-in-the-preview-window`
- `audit-options-thumbnails-interaction-only-on-files-with-no-duration`
- `audit-options-thumbnails-interaction-on-shift-selection-focus-thumbnails-in-the-preview-window`
- `audit-options-thumbnails-interaction-only-on-files-with-no-duration-2`

**Recommendation:** Retain four scoped first-pass proposals, conditional on central execution/render review.

Preferences are independent saved flags with child enabled gates; selection model separates focus publication from range anchor/ghost selection. Actual source gates singleton Some(0) versus None and collection positive aggregate duration, and main reselect checks binding/visible ownership. `tests/model/thumbnail_preview_selection.rs:43,153,203` compare all 16 Qt option policies, 192 exact modifier/range/key focus/anchor/ghost states and collection-duration shapes. `tests/gui/thumbnail_preview_selection.rs:102` delivers actual pointer/key modifiers, applies Options only on acceptance, checks owned preview pixels/viewing statistics and hidden/closed/rebound retirement; `:339` uses real Store collections and zero-duration distinctions. Recorder deliberately disconnects downstream movie playback but records genuine selection/focus publications. Native playback/movie parity is therefore not inferred from this slice.

## viewer-tag-wheel.json

IDs:

- `audit-options-media-viewer-hovers-hover-windows-allow-a-mouse-wheel-scroll-over-the-taglist-to-propagate-to-the-main-canvas`

**Recommendation:** Retain scoped first-pass proposal (1), conditional on central execution/render review.

`WheelGate` preserves the reference strict .57-second delay, reversal, new-media grace and 250-second direction reset. `viewer_tag_wheel.rs::bind` first scrolls the measured actual list then gates navigation/Ctrl zoom only at an unconsumed edge, rereads preferences per event and checks independent CanvasTracker lifetime. Model tests compare 48 actual Qt gates to `viewer_tag_wheel.json`. `tests/gui/viewer_tag_wheel.rs:51` proves long-list offset changes precede navigation, immediate/never/delayed/no-scrollbar behavior, media transitions, zoom and retired callbacks; `viewer_drag.rs::two_visible_viewers_read_saved_preferences_and_keep_independent_live_lifetimes` covers older still-visible viewer ownership. Winit’s normalized wheel units/native font metrics are disclosed implementation differences; no full hover tag-list/context selection parity is claimed.

## write-tags-native-components.json

IDs:

- `Support-only native supplement; no reference IDs claimed.`

**Recommendation:** Retain support-only native hierarchy supplement; reference completion credit 0; preserve Partial domain/search/relationship/review families.

Exported `write_autocomplete.slint::{WriteTagInput,WriteTagsWindow}` and `write_tag_window::open/open_favourites` are concrete shared components. Existing `tests/gui/write_autocomplete.rs:162,286,443,591,697` compare real `write_tag_autocomplete.json` results, staged child Apply/Cancel and retired actions, favourite questions, actual weak main search/duplicate launches, domain counts/custom-child cancellation; later tests assert real text paste/focus/selection consumers. `tests/gui/options_window.rs::favourite_tags_child_replays_reference` proves the caller/Options transaction. Model tests compare counted suggestions, literal tags, decorations and context choices. The supplement’s first-pass feature-group rows do not count as new reference IDs/leaves. Native domains, multiple-tag page menus, remote maintenance, complete review selection/menu topology remain explicitly Partial. One caveat is stale anchors: current `write_tag_window.rs` grew (open_favourites is :67 rather than older :45), so central inventory repinning must resolve original immutable commits instead of interpreting packet line numbers as current-source locations.
