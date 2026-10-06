## archive-delete-policies

Manifest: `docs/rust/gui-coverage/parity/archive-delete-policies.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-files-and-trash-when-finishing-archive-delete-filtering-always-delete-from-all-possible-domains` | Retain conditional proposal; CI/render pending |
| `audit-options-files-and-trash-when-finishing-archive-delete-filtering-delay-activation-of-multiple-deletion-choice-buttons` | Retain conditional proposal; CI/render pending |

Retain two concrete control proposals. Python `FilesAndTrashPanel.py:40,44,162`, canvas `ClientGUICanvas.py:3941` and finish panel `ClientGUIScrolledPanelsCommitFiltering.py:204` supply options, choices and 1.2-second delay. Executed recorder/fixture `oracle/record_archive_delete_policies.py` / `fixtures/archive_delete_policies.json` cover four policy states, 48 alternatives and Forget. Native model `crates/hydrus-gui-model/src/archive_delete.rs:224,410` deduplicates full current/deleted contexts and validates captured service keys inside content transaction; native `archive_delete_window.rs` guards deadline, Main and exact child. Model tests `tests/model/archive_delete_policies.rs:53,138,285` assert scoped settings merge, recorded alternatives and replaced identity; GUI `tests/gui/archive_delete_policies.rs:83,178,263,317` assert predeadline rejection, real selected-domain membership, nested Forget, Cancel/rebind and destroyed Main. No GUI Store-error presentation regression; source/model identity failure is covered. Deterministic secondary-domain order and native modal-layer topology are disclosed. Parent stays Partial.

## command-palette

Manifest: `docs/rust/gui-coverage/parity/command-palette.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-command-palette-command-palette-initially-show-all-page-results` | Retain prior validated ID; current-head checks pending |
| `audit-options-command-palette-command-palette-initially-show-page-history-results` | Retain prior validated ID; current-head checks pending |
| `audit-options-command-palette-command-palette-initially-show-favourite-search-results` | Retain prior validated ID; current-head checks pending |
| `audit-options-command-palette-command-palette-start-searching-when-this-many-characters-have-been-typed` | Retain prior validated ID; current-head checks pending |
| `audit-options-command-palette-command-palette-max-page-results-to-show` | Retain prior validated ID; current-head checks pending |
| `audit-options-command-palette-command-palette-max-page-history-to-show` | Retain prior validated ID; current-head checks pending |
| `audit-options-command-palette-command-palette-max-favourite-searches-to-show` | Retain prior validated ID; current-head checks pending |
| `audit-options-command-palette-command-palette-include-page-of-pages-page-results` | Retain prior validated ID; current-head checks pending |
| `audit-options-command-palette-command-palette-open-favourite-searches-in-a-new-page` | Retain prior validated ID; current-head checks pending |
| `audit-options-command-palette-command-palette-advanced-search-main-menubar` | Retain prior validated ID; current-head checks pending |
| `audit-options-command-palette-command-palette-advanced-search-media-menu` | Retain prior validated ID; current-head checks pending |
| `audit-options-command-palette-command-palette-search-provider-order-ordered-editor-list` | Retain support/parent only; zero leaf credit |
| `audit-options-command-palette-command-palette-search-provider-order-add` | Retain prior validated ID; current-head checks pending |
| `audit-options-command-palette` | Retain Partial; zero credit |
| `audit-options-command-palette-command-palette` | Retain Partial; zero credit |
| `audit-options-command-palette-command-palette-search-provider-order` | Retain Partial; zero credit |

Retain 12 previously validated control IDs; retain ordered-list support assessment without leaf credit and three Partial parents. Real Qt `oracle/record_command_palette.py` / fixture covers providers/options. `crates/hydrus-gui-model/src/command_palette.rs:89,263,276,300` implements policy filtering and query/owner invalidation. GUI `tests/gui/command_palette.rs:47,189,246,328,370,450` asserts actual page/favourite/menu activation, stale workers/owners, removed providers, calculator and Unicode thresholds; `tests/gui/options_window.rs:4337` covers staged queue/settings Apply/Cancel/reopen. Ordered queue `ui/palette_provider_queue.slint:21` has extended click/Delete but lacks generic arrow selection; keep this explicit, and do not count that structural node. Calculator rounding/bounds, matched-text/icons and native command availability retain parent limitations. Preserve prior checkpoint accounting; current-head CI/render is separately pending.

## database-locations

Manifest: `docs/rust/gui-coverage/parity/database-locations.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-media-database-locations-paths` | Demote to Partial; zero new credit |
| `audit-media-database-locations-max-size` | Demote to Partial; zero new credit |
| `audit-media-database-locations-rebalance` | Demote to Partial; zero new credit |
| `audit-media-database-locations-thumbnails` | Demote to Partial; zero new credit |

Demote all four new completion claims. Reference `hydrus/client/gui/panels/ClientGUIFilesPhysicalStoragePanels.py:471,1128,1291,1339` owns list/runtime/max-size/thumbnail flows. No executed reference recording is provided. `crates/hydrus-gui/src/database_locations_window.rs:336` retained max-size Apply only upgrades the child; Cancel at :356 merely hides it. Apply still writes at :352 after child Cancel or parent close. Parent action :267 and remove/clear-thumbnail accepted callbacks similarly lack retirement/current-owner guards; custom runtime :511 can start a worker from a closed retained child. These are concrete lifecycle defects. Store `storage_locations.rs:406,441` moves physical prefix before DB update and polls cancellation between moves; tests :471,493 cover ideal weights and successful empty-directory rebalance only. Model `tests/model/database_locations.rs:28` checks manually authored rows/buttons, not native cancel, max persistence, thumbnail override, physical contents, move failure/recovery. Cancellation observed by progress callback `database_locations_window.rs:549` is followed by one more move (`storage_locations.rs:447,456`), so stop semantics need an explicit regression. Granularity now exists despite the packet's stale limit; no extra credit.

## debug-delayed-popup

Manifest: `docs/rust/gui-coverage/parity/debug-delayed-popup.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-help-debug-action-make-a-popup-in-five-seconds` | Retain conditional proposal; CI/render pending |

Retain one new concrete proposal. Python QAction `ClientGUI.py:3644` uses real CallLater(5)/ShowText; executed `oracle/record_debug_delayed_popup.py` records genuine overlapping deadlines and hidden-main publication. Native `crates/hydrus-gui/src/debug_long_popup.rs:121,169,275` partitions due deadlines and publishes separate done jobs, with weak/retired binding guards. GUI `tests/gui/debug_delayed_popup.rs:49,138,177` assert no early publication, separate keys, reopen persistence, long producer coexistence, Exit Cancel and accepted-exit/rebind/final-owner destruction. The deterministic native clock proves nominal deadlines, not observed physical event-loop delivery or OS rendering. Separate from the unsupported modal debug actions; zero parent credit.

## duplicate-colours

Manifest: `docs/rust/gui-coverage/parity/duplicate-colours.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-duplicates-colours-background-light-dark-switch-intensity-for-a` | Retain conditional proposal; CI/render pending |
| `audit-options-duplicates-colours-background-light-dark-switch-intensity-for-b` | Retain conditional proposal; CI/render pending |
| `audit-options-duplicates-colours-draw-image-transparency-as-checkerboard-in-the-duplicate-filter` | Retain conditional proposal; CI/render pending |

Retain three new narrow controls. Executed Qt `oracle/record_duplicate_colours.py` / fixture has five Options cases, 143 QColor cases and eight canvas cases. `crates/hydrus-gui-model/src/duplicate_colours.rs:6,24,108` mirrors HSV adjustment and transparent checkerboard/greenscreen policy. Actual consumer `crates/hydrus-gui/src/filter_window.rs:178` loads saved colours and sets A/B background/transparency. Model `tests/model/duplicate_colours.rs:24,72` compares recorded outputs and concurrent-background preservation; GUI `tests/gui/duplicate_colours.rs:56` asserts parent Cancel/retained Apply/normalized persistence. In-source native consumer regression `filter_window.rs:897` separately asserts switching, rendered pixels and successor isolation. Detached Qt draft is not full modal Options Cancel evidence; actual native owner Cancel is separately authored. Keep styling/topology caveats.

## export-default-directory

Manifest: `docs/rust/gui-coverage/parity/export-default-directory.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-exporting-export-folder-default-export-directory` | Retain prior validated ID; current-head checks pending |

Retain prior validated leaf, no new completion. Executed `oracle/record_export_default_directory.py` / fixture covers real path editor and manual export. Model `export_files.rs:82` resolves configured/native relative paths and HOME/USERPROFILE fallback. GUI `tests/gui/options_window.rs:4157` asserts picker Cancel, parent Cancel/retained browse+Apply, configured filename destinations, one-off independence, relative paths and reopened settings. Model `tests/model/options_dialog.rs:2008` and Store import `src/import/decode.rs:5841` compare literal/portable cases. Missing-directory/permission failures are not exercised by this setting test; Windows fallback is source-supported. Native stores resolved paths versus Qt portable text. Existing checkpoint evidence is distinct from pending current-head validation.

## files-trash

Manifest: `docs/rust/gui-coverage/parity/files-trash.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-files-and-trash-confirm-sending-more-than-one-file-to-archive-or-inbox` | Retain prior validated ID; current-head checks pending |
| `audit-options-files-and-trash-confirm-sending-files-to-trash` | Retain Partial; zero credit |
| `audit-options-files-and-trash-advanced-file-deletion-and-custom-reasons-use-the-advanced-file-deletion-dialog` | Retain prior validated ID; current-head checks pending |
| `audit-options-files-and-trash-advanced-file-deletion-and-custom-reasons-remember-the-last-action` | Retain prior validated ID; current-head checks pending |
| `audit-options-files-and-trash-advanced-file-deletion-and-custom-reasons-remember-the-last-reason` | Retain prior validated ID; current-head checks pending |
| `audit-options-files-and-trash-advanced-file-deletion-and-custom-reasons-ordered-editor-list` | Retain Partial; zero credit |
| `audit-options-files-and-trash-advanced-file-deletion-and-custom-reasons-add` | Retain prior validated ID; current-head checks pending |
| `audit-options-files-and-trash-advanced-file-deletion-and-custom-reasons-edit` | Retain prior validated ID; current-head checks pending |
| `audit-options-files-and-trash-advanced-file-deletion-and-custom-reasons` | Retain Partial; zero credit |

Retain six previously validated concrete controls; retain confirm-trash, ordered-list and parent Partial. Real Qt `oracle/record_files_trash.py` / fixture covers counts, reasons/actions and raw queue decisions. Model `crates/hydrus-gui-model/src/delete_files.rs` creates local/physical/clean plans; native `delete_files_window.rs` and `options_deletion.rs` own staged children/queue. Model `tests/model/delete_files.rs:34,123,203,269,307` asserts recorded selected reasons, locks/clean preservation, concurrent merge, duplicate/empty/raw queue text, physical-only remembered keys and mixed reasons. GUI `tests/gui/media_actions.rs:240,309` asserts confirmation and viewer-close child cancellation; `tests/gui/options_window.rs:4870,4979` covers Apply/Cancel/reopen and real delete consumers. Remote/simple picker/undelete and generic arrow-selection gaps remain explicit, correctly preventing parent/list promotion. Store-error UI is not newly tested. No second completion credit.

## force-idle-mode

Manifest: `docs/rust/gui-coverage/parity/force-idle-mode.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-help-debug-action-force-idle-mode` | Retain Partial; zero credit |

Retain Partial/count=false. Real Qt `oracle/record_force_idle_mode.py` / fixture records debug/controller matrix. `crates/hydrus-gui/src/force_idle.rs:16,61,78` attaches binding-owned Monitor override, leaving activity timestamps/deadlines intact and rejecting hidden/prompt/retired input. GUI `tests/gui/force_idle.rs:51,190,267` asserts checked state, live autosave/trash/physical consumers, unchanged deadlines, Exit Cancel, rebind and final-clone retirement. Worker wake/CPU and whole-runtime behavior remain missing, so zero completed leaf credit.

## help-debug-actions

Manifest: `docs/rust/gui-coverage/parity/help-debug-actions.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-help-debug-action-flush-log` | Retain conditional proposal; CI/render pending |
| `audit-options-help-debug-action-force-database-commit` | Demote to Partial; zero new credit |
| `audit-options-help-debug-action-show-env` | Retain conditional proposal; CI/render pending |
| `audit-options-help-debug-action-simulate-program-exit-signal` | Retain conditional proposal; CI/render pending |
| `audit-options-help-debug-action-make-a-qmessagebox` | Demote to Partial; zero new credit |
| `audit-options-help-debug-action-make-a-modal-popup-in-five-seconds` | Demote to Partial; zero new credit |
| `audit-options-help-debug-action-make-a-non-cancellable-modal-popup-in-five-seconds` | Demote to Partial; zero new credit |
| `audit-options-help-debug-action-make-some-popups` | Demote to Partial; zero new credit |
| `audit-options-help-debug-action-reset-multi-column-list-settings-to-default` | Demote to Partial; zero new credit |
| `audit-options-help-debug-action-save-last-session-gui-session` | Retain conditional proposal; CI/render pending |
| `audit-options-help-debug-action-clear-all-rendering-caches` | Demote to Partial; zero new credit |
| `audit-options-help-debug-action-what-is-this` | Retain conditional proposal; CI/render pending |

Demote seven new claims listed below; retain five only as conditional narrow source proposals. Python `ClientGUI.py:1322` publishes modal_message at :1332; native `crates/hydrus-gui/src/debug_actions.rs:73` publishes ordinary toaster jobs with an unowned sleeping worker. Both modal leaves lack defining modality. Python reset :1595/:1606 calls ResetToDefaults; native :124 accepted callback is empty. QMessageBox is modal in Python :3645; native message_then :57 opens an ordinary parentless chooser (`choice_buttons.rs:25`) without main input guard. Some-popups omits callback/network/subjob behaviors; model test `tests/model/debug_actions.rs:13` explicitly tests the reduced set. ForceCommit at native :136 executes wal_checkpoint(PASSIVE) inside Store.write; `crates/hydrus-store/src/conn.rs:397,405,443` executes callback inside BEGIN IMMEDIATE/SAVEPOINT, where SQLite checkpoint is locked. It cannot perform intended checkpoint. Cache callback `crates/hydrus-gui/src/lib.rs:2948` clears owned image/thumbnail caches only, versus Python `ClientController.py:694` also clearing tile/graphics-view families; no native debug-cache consumer assertion. Model `tests/model/debug_actions.rs:13,65` and main_menu assert strings/job construction/order, not actual writer/dialog/cache/exit/session boundaries. `DIFFERENCES.md:3543` already acknowledges most omissions. Retained flush/env/exit/session/information proposals have clear native dispatch, but lack this packet's native consumer regression; do not call them fully validated.

## import-options

Manifest: `docs/rust/gui-coverage/parity/import-options.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `subscriptions-copy-options` | Retain prior validated ID; current-head checks pending |

Retain prior validated subscriptions-copy-options; no new leaf/alias credit. Actual Qt `oracle/record_subscription_import_options.py` / fixture covers menu bindings and paste/clear decisions. Model `tests/model/subscription_import_options.rs:8` compares serialized outputs for all modes and frozen selected keys. Native `subscriptions_window.rs:1565` decodes/stages clipboard; `import_options_overwrite_window.rs:60` uses terminal active guard. GUI `tests/gui/subscriptions.rs:19,149` asserts favourite/custom overwrite, child/parent Cancel, stale handles, clipboard round trip, persistence and saved settings. Serialized subscription exchange remains separate/incomplete. Prior checkpoint retained; current-source native/CI pending.

## login-workflows

Manifest: `docs/rust/gui-coverage/parity/login-workflows.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `login-credential-definition` | Retain prior validated ID; current-head checks pending |
| `login-credentials` | Retain prior validated ID; current-head checks pending |
| `login-scripts` | Retain Partial; zero credit |
| `login-script` | Retain Partial; zero credit |
| `exchange-login` | Retain conditional proposal; CI/render pending |
| `login-step-content` | Retain conditional proposal; CI/render pending |
| `login-step` | Retain support/parent only; zero leaf credit |
| `logins` | Retain Partial; zero credit |
| `login-test-results` | Retain prior validated ID; current-head checks pending |

Retain five scoped concrete proposals (some are already in prior checkpoint), keep login-step parent/support at zero leaf credit and login-script/login-scripts/logins Partial. Real Qt login editor/domain/argument/cookie/results recordings are present; `oracle/record_login_editors.py` is the initial editor recording, not proof of the entire HTTP manager. Native `login_credential_window.rs:43,174` has terminal active guards; step children own dictionaries/cookies and restricted VARIABLE/VETO parser route. `login_test_window.rs:228` constructs actual engine with fresh test cookies and streamed result events. GUI `tests/gui/login_workflows.rs:36,98,234,366,469` asserts mask/advisory prompts, nested Cancel, domain reset/activation, recorded result review and full Unicode copy; :627,798,898,996 exercises real HTTP and persisted session consumers, :1144,1237,1394,1480 sequential dictionary/cookie list boundaries. `crates/hydrus-net/tests/login.rs:192,251,456` asserts token/cookie/VETO/401, active cancellation and partial finished results. Mixed-login registered package tests are in codec/downloader_interchange modules and preserve domain credentials/activation/identity with concurrent-write guards; old direct login-only tests cannot substitute. Help/HTML, cookie expiry/raw display and broader child limits remain; no parent completion.

## media-closing

Manifest: `docs/rust/gui-coverage/parity/media-closing.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-media-viewer-closing-focus-when-closing-the-media-viewer-re-select-original-search-page` | Retain prior validated ID; current-head checks pending |
| `audit-options-media-viewer-closing-focus-when-closing-the-media-viewer-tell-original-search-page-to-select-exit-media` | Retain prior validated ID; current-head checks pending |
| `audit-options-media-viewer-closing-focus-advanced-when-closing-the-media-viewer-with-the-above-focusing-options-activate-main-gui` | Retain prior validated ID; current-head checks pending |
| `audit-options-media-viewer-closing-focus-debug-when-closing-the-media-viewer-at-any-time-activate-main-gui` | Retain prior validated ID; current-head checks pending |

Retain all four prior validated option leaves; no new completion. Reference `hydrus/client/gui/panels/options/MediaViewerPanel.py:125` names four policies; executed `oracle/record_viewer_closing_options.py` / fixture records close notifications/activation order. Native `crates/hydrus-gui/src/viewer_closing.rs:70` loads current saved policy against frozen weak original page/Main; :89 changes exact page/exit selection, :117 requests native activation. Model `tests/model/viewer_closing.rs:7` compares fixture order, unowned/no-media behavior and all policy combinations. GUI `tests/gui/options_window.rs:3135,3338` verifies actual selection, captured original page, Options Cancel/reopen, stale replaced session/viewer and activation observer; in-source Owner test covers destroyed original/Main. Observer records real activation attempt, not compositor focus success. Preserve checkpoint; native OS focus/render on current head remains centrally pending.

## media-view-options

Manifest: `docs/rust/gui-coverage/parity/media-view-options.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-media-playback-per-filetype-handling-add` | Demote to Partial; zero new credit |
| `audit-options-media-playback-per-filetype-handling-edit` | Demote to Partial; zero new credit |
| `audit-options-media-playback-per-filetype-handling-delete` | Demote to Partial; zero new credit |

Demote three new completion claims. Static executed `oracle/dump_media_view_options.py` / fixture records rows, filetypes and capabilities, not interaction/cancel/consumer behavior. Native `crates/hydrus-gui/src/options_media_views.rs:154` Apply has no active/visibility/identity guard; Cancel :165 merely hides. Accept :267 checks only parent active: retained cancelled old edit can overwrite current parent draft and persist on parent Apply. Add chooser :311 can open an orphan editor after parent Cancel without parent guard. Parent retirement blocks eventual save but does not fix same-parent child Cancel. Model `tests/model/media_view_options.rs:7,81` compares static choices and invokes table add/delete; no native per-filetype options test exercises child Cancel, Options Apply/reopen or inherited saved viewer/preview consumers. Delete's class protection is asserted, but full UI/persisted-consumer claim is unsupported. Fixture normalizes QtMediaPlayer to mpv; runtime availability remains a documented difference (`DIFFERENCES.md:3527`).

## network-controls

Manifest: `docs/rust/gui-coverage/parity/network-controls.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-network-download-control-cog` | Retain prior validated ID; current-head checks pending |
| `audit-network-download-control-errors` | Prior validated ID; current-source ownership repair required |
| `audit-network-file-log-clipboard-import` | Retain prior validated ID; current-head checks pending |
| `audit-network-file-log-search-urls` | Retain prior validated ID; current-head checks pending |
| `audit-network-file-log-png` | Retain prior validated ID; current-head checks pending |
| `audit-network-file-log-advanced` | Retain prior validated ID; current-head checks pending |
| `audit-network-search-log-exchange` | Retain prior validated ID; current-head checks pending |
| `audit-network-quality-review` | Retain prior validated ID; current-head checks pending |
| `audit-network-quality-csv` | Retain prior validated ID; current-head checks pending |
| `audit-network-pause-boot` | Retain prior validated ID; current-head checks pending |
| `audit-network-matcher-favourites` | Retain prior validated ID; current-head checks pending |
| `audit-network-parser-auto-links` | Retain prior validated ID; current-head checks pending |
| `url-links-parser` | Retain prior validated ID; current-head checks pending |
| `url-domain` | Retain prior validated ID; current-head checks pending |
| `url-preview` | Retain prior validated ID; current-head checks pending |

Preserve all 15 prior validated IDs and existing checkpoint credit, but errors has a confirmed current-source ownership defect requiring repair. `crates/hydrus-gui/src/network_job_control.rs:80` Errors::show replaces A with B; A's captured close closure at :89 blindly takes the shared slot and hides B. Retained A.close-clicked after owner cancel/reopen does the same. GUI `tests/gui/network_job_control.rs:211` closes A before clearing and never retains A across B, so cannot catch this. Separate proposed network-errors.patch fixes exact-owner close and authors a focused successor/Cancel regression; not applied here.

Cog uses captured epoch/request (`network_job_control.rs:329,360`), live engine/page override tests `tests/gui/network_job_control.rs:59,253,366,498`, and actual Qt fixture. File-log clipboard/search/PNG/advanced tests `tests/gui/file_log.rs:192,267,375,461` assert persistent imports, duplicate/status preservation, actual OR query results, picker Cancel/malformed PNG, successful last path and normalization consent; model `tests/model/file_log.rs:133,155,178,275,288` compares reference objects. Search-log `tests/model/search_log.rs:116`, GUI `tests/gui/search_log.rs:177` assert complete object transport and duplicate/continuation choices. Quality uses model `subscription_quality.rs:23,42,60`; GUI `tests/gui/edit_subscription.rs:288` compares actual fixture messages/clipboard, saved-only selection, current counts, no draft save, close cancellation and missing-log failure (:422,:438,:464,:485). Boot-pause menu `tests/gui/menu_bar.rs:540` checks persistence/live resume; GUI/CLI apply at startup, secondary engine construction does not reapply preference.

Regex popup `tests/gui/regex_favourites.rs:269` covers global live refresh, Apply/Cancel and owner retirement. Parser auto-link `tests/gui/parser_editors.rs:1658` reproduces genuine reference owner no-op; :1802 verifies ordered chooser/clear/Cancel/resolver. URL-domain/preview GUI `tests/gui/downloader_definitions.rs:569,791` asserts raw/regex sequential prompt Cancel, invalid values, actual saved masks and five read-only outputs. Native nonmodal/error presentation and bounded carrier remain disclosed. Current-head hosted/render verification is pending separately from old checkpoint.

## or-label-connector

Manifest: `docs/rust/gui-coverage/parity/or-label-connector.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-tag-presentation-other-rendering-or-connecting-string-on-one-line` | Retain conditional proposal; CI/render pending |

Retain one new finite editor proposal. Reference `TagPresentationPanel.py:200,336` edits/saves raw string; use at `ClientSearchPredicate.py:1404` is within disabled custom rendering code, so keeping live OR labels unchanged matches reference. Real Qt `oracle/record_or_connector.py` / fixture includes raw blank/whitespace/Unicode/newline plus unchanged label/copy outputs. Native Store `src/or_connector.rs:32,44` imports/backfills raw strings with native precedence. Model `tests/model/or_connector.rs:36` checks Cancel, field isolation, Store reopen and immutable query/copy; GUI `tests/gui/or_connector.rs` covers actual Options control, live OR query/colour and retained-owner rejection. No OR layout/renderer parent promotion.

## predicate-custom-defaults

Manifest: `docs/rust/gui-coverage/parity/predicate-custom-defaults.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-predicate-custom-defaults` | Retain Partial; zero credit |

Retain Partial/count=false. Real Qt recorder/fixture covers 40 panel families/1600 comparability pairs. Native model `predicate_editors/defaults.rs:24,51,69` implements reference comparability, family replacement, empty no-op and future-only reset; initialise.rs reverses canonical predicates with explicit predicate precedence. Star-save intentionally bypasses separate regex acceptance validation. GUI custom-default tests in `tests/gui/predicate_editors.rs`, including imported-default durability :1316, assert immediate save surviving Cancel, comparable reset, recents separation and durable imported clearing. URL-class stable identity across rename, existing precision and UI topology limits prevent completion. No new leaf credit; current CI/native render pending.

## read-or-native-components

Manifest: `docs/rust/gui-coverage/parity/read-or-native-components.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

ID: `catalog.searchorwindow`; native hierarchy support only, zero original-reference leaf credit.

Retain hierarchy supplement Partial/support-only. catalog.searchorwindow is a native catalog ID, not an original reference leaf; zero count. Real basic/advanced recorders `oracle/record_read_or.py`, `record_read_or_editors.py` and fixtures exist. Native `search_or_window.rs:47,67,264,470,572` guards caller, recursively cancels nested/system slots and retires owners. GUI `tests/gui/read_or.rs:44,182` replays draft/preview/query behavior; :318 verifies retained callbacks and :326 recursively cancels nested/system children. Older packet anchor line positions moved, but owned-child structure remains. No parent/alias promotion.

## selected-deletion-records

Manifest: `docs/rust/gui-coverage/parity/selected-deletion-records.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-media-context-missing-clear-deleted` | Retain conditional proposal; CI/render pending |

Retain one new proposal. Real Qt `oracle/record_selected_deletion_records.py` / fixture covers current selection at QAction dispatch, physical-local deletion eligibility, collections and changes during question. Native model `selected_deletion_records.rs:18,46` captures ordered eligible IDs once and commits chunks(64) independently. Main dispatch uses binding/page departure epoch and advanced-child guards. Model `tests/model/selected_deletion_records.rs:9,69` compares fixture states and injects a real batch-two trigger failure, proving committed prefix plus remaining records/physical queue. GUI `tests/gui/selected_deletion_records.rs:48,205,270` verifies labels/questions, actual membership and importer known_status (:195), hidden/rebind/tab roundtrip/child guards and Store-error warning UI. No physical deletion or queue removal, no duplicate global-clear credit.

## shortcut-sets

Manifest: `docs/rust/gui-coverage/parity/shortcut-sets.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-shortcuts-built-in-hydrus-shortcut-sets-edit` | Retain conditional proposal; CI/render pending |
| `audit-options-shortcuts-built-in-hydrus-shortcut-sets-restore-defaults` | Demote to Partial; zero new credit |
| `audit-options-shortcuts-custom-user-sets-add` | Retain conditional proposal; CI/render pending |
| `audit-options-shortcuts-custom-user-sets-delete` | Demote to Partial; zero new credit |
| `audit-options-shortcuts-custom-user-sets-edit` | Retain conditional proposal; CI/render pending |

Retain built-in Edit and custom Add/Edit as narrow new editor proposals; demote restore-defaults/Delete due to missing native boundary evidence. Reference `ShortcutsPanel.py:134,162,174,211,283` owns actual decisions; static `oracle/dump_shortcut_sets.py` records only defaults/descriptions. Model `tests/model/shortcut_sets.rs:22,44,84` compares rows/107 bindings then calls save/delete/restore directly with handcrafted questions. Current native `shortcut_windows.rs:217,249,284,591` stages actions with parent/set guards. Later GUI `tests/gui/shortcut_capture.rs:38,129,169` supplies built-in/custom editor Apply/Cancel/reopen and main/viewer consumers, repairing some old model-only evidence. Actual restore Missing/Replace chooser cancellation and custom multi-delete confirmation/persisted reopen have no native regression. Custom-set viewer activation remains absent and no family credit follows. Native prevents built-in-name collisions (model test :104); Python de-dupes only custom names (`ShortcutsPanel.py:151,198`), a documented naming difference rather than exact GetNonDupeName parity.

## sidecar-router-import

Manifest: `docs/rust/gui-coverage/parity/sidecar-router-import.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-shared-sidecar-import` | Retain conditional proposal; CI/render pending |

Retain one new import proposal. Real Qt `oracle/record_router_import.py` / fixture covers nested permitted subsets in both contexts, all-refused, ordered PNG prefix/later decode error and picker Cancel. `crates/hydrus-gui/src/router_import_window.rs:85` requires terminal active/visible captured owner, stops on first failed selected PNG and retains earlier reports for explicit review acceptance; compatible type/direction filtering preserves router data. Codec router tests compare actual fixture subsets/type warnings; `tests/gui/sidecars.rs` import tests assert review/parent Cancel/replacement/drop boundaries and persisted manual-export sidecar consumer. Native review Cancel discards successful prefix, versus Qt immediate insertion into its unsaved queue: this extra stage is explicitly disclosed. Zero sibling-export/structural credit.

## tab-menu-selectable-pages

Manifest: `docs/rust/gui-coverage/parity/tab-menu-selectable-pages.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-tabs-context-action-2120-selectable-page` | Demote to Partial; zero new credit |

Demote new leaf for insufficient final evidence. Reference `ClientGUIPages.py:2103` chooses clicked notebook, labels via GetNameForMenu at :2118 and invokes ShowPage :2120. Native `crates/hydrus-gui/src/pages.rs:2130` traverses appropriate subtree and produces exact key/name-summary menu. Cited GUI-binary test `tests/gui/session.rs:2736` only creates Pages and inspects menu, never creates Main or dispatches selection; :2778 strips label before first " - ", removing asserted count/progress elision. Only names/keys are compared at :2792. No executed reference recording. Source supports implementation, but exact labels and actual selecting consumer are not proven by cited assertions.

## tag-namespace-order

Manifest: `docs/rust/gui-coverage/parity/tag-namespace-order.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-tag-sort-tag-sort-namespace-grouping-sort-add` | Retain conditional proposal; CI/render pending |
| `audit-options-tag-sort-tag-sort-namespace-grouping-sort-edit` | Retain conditional proposal; CI/render pending |

Retain two new narrow Add/Edit proposals; structural queue/Paste/group remains Partial. Real Qt `oracle/record_tag_namespace_order.py` / fixture has 18 raw queue paths, six sorts and three real modal EnterText cases. Native `options_tag_namespace_order.rs:62,76` guards terminal owner/current child; model keeps stable duplicate IDs and preserves raw blank/colon/case/whitespace. Model `tests/model/tag_namespace_order.rs:18,122` compares recorded raw states/sorts and concurrent field preservation. GUI `tests/gui/tag_namespace_order.rs:75,246,331` asserts exact prompt/parent transaction, saved real sidebar sort and hidden/rebound/accepted-close rejection. Viewer shares unchanged core sorter; this new consumer regression explicitly exercises sidebar only, no new independent viewer paint attestation.

## thumbnail-file-relationships

Manifest: `docs/rust/gui-coverage/parity/thumbnail-file-relationships.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-media-context-missing-duplicates` | Demote to Partial; zero new credit |

Demote broad missing-duplicates leaf. Reference `ClientGUIMediaMenus.py::AddDuplicatesMenu` also has custom merge/collection relationships and correct king/domain availability. Native model `file_relationships.rs:100,145`, thumbnail `thumbnail_menu.rs:859` and Main dispatch `lib.rs:4598,5041` provide substantive subset. Manifest omits custom merge/collections and viewer; `DIFFERENCES.md:3557` additionally discloses king-outside-domain handling. Model `tests/model/file_relationships.rs:29,53,88` manually constructs labels and calls Store helpers (alternates/king/dissolve/false positives); no executed Qt recording, actual thumbnail dispatch, confirmation Cancel, stale callbacks or custom-merge consumer test. That subset does not justify whole original leaf completion.

## viewer-drag-anchor

Manifest: `docs/rust/gui-coverage/parity/viewer-drag-anchor.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-media-viewer-mouse-behaviour-anchor-mouse-cursor-during-media-viewer-drags` | Retain conditional proposal; CI/render pending |
| `audit-options-media-viewer-mouse-behaviour-if-set-to-anchor-drags-undo-on-apparent-touchscreen-drag` | Retain conditional proposal; CI/render pending |

Retain two new narrow settings. Python canvas `ClientGUICanvas.py:3207` computes >50 Manhattan touch latch and reads current global saved flags, requests warp at :3231 then moves media :3240. Real Qt `oracle/record_viewer_anchor_options.py` records requested warps and real movement, not compositor execution. Native model `viewer_drag.rs:30` mirrors history; GUI source `viewer_drag.rs:15,36,43,59` checks independent CanvasTracker/visibility, reloads preferences and requests Winit warp. GUI `tests/gui/viewer_drag.rs:42,186` sends actual pointer moves, asserts recorded deltas, Options Apply/Cancel/reopen, multiple still-visible viewers and retired re-shown rejection. Model asserts exact threshold/warp requests. OS refusal remains unverified, broader touch/mouse/preview families Partial.

## viewtime-milliseconds

Manifest: `docs/rust/gui-coverage/parity/viewtime-milliseconds.json`. Reviewed source: `3702cdbee99473cfd7af073aa044fe847f78f405`. No builds, test execution, source/git edits or mutation testing. Central exact-source CI/native render review remains pending. Paths below are repository-relative; unqualified source/test files are in the crate named in the paragraph.

| ID | Recommendation |
| --- | --- |
| `audit-options-predicate-file-viewing-statistics-fileviewingstatsviewtime-test` | Retain prior validated ID; current-head checks pending |

Retain prior validated leaf; no new credit. Real Qt editor/DB `oracle/record_viewtime_milliseconds.py` / fixture has 96 operator/threshold/canvas cases including 1001ms float-to-integer boundary. Core from_viewtime_milliseconds, legacy viewtime_value, editor and search planner preserve canonical typed units/conversion. Model `tests/model/viewtime_milliseconds.rs:49` reconstructs real editor fields, compares imported typed predicate/text and actual planner query hashes (:133), asserts 96 cases (:136) plus whole-second serde compatibility. GUI `tests/gui/predicate_editors.rs:654` asserts accepted 345/1001ms recents, reopened display, Cancel/retained OK rejection. Label at :724 is intentionally pretty-time 1.0 seconds, while saved value remains 1001ms. Free-text whole seconds and arbitrary sub-ms legacy rounding stay excluded. Prior checkpoint retained; current native/CI/render pending.
