# Final five-leaf review: ff858ded14518d4a9a1d39bb5aafe0213c992983

All five assigned IDs are eligible for bounded Linux sign-off. Run37428534955 required jobs succeeded; saved logs independently show the exact checkout and all relevant GUI/model/store tests passing. GUI suite:696 passed,0 failed,0 ignored. Windows/macOS are deferred; no parent credit.

Fresh artifact11396734519: zip SHA-256 `baa000def90c94892745407a1204dc868833361f3ebd7e32b941f2f1a2ab594f` matches the upload log. Four native exports were actually viewed and individually verified against the266-image manifest. Three Qt PNGs were also viewed. Source/reference hashes match the exact git object.

## popup-width

- Eligible: `audit-options-popup-notifications-popup-window-toaster-approximate-max-width-of-popup-messages-in-characters`
- Eligible: `audit-options-popup-notifications-popup-window-toaster-bugfix-force-this-width-as-the-fixed-width-for-all-popup-messages`

- Viewed the full native Main capture: two bottom-right cards are visible. The old long synthetic text wraps within a narrower card; the short successor fills a wider fixed-width card. Both texts and the two-messages summary are readable, with no selected-card clipping observed.
- Qt reference shows a single narrow long wrapped card, with updated synthetic words rather than the shorter repeated native synthetic words. Its successor was constructed under a separate host: JSON live.after/successor measurements and green native geometry assertions establish retained old cap/new fixed cap. No two-card Qt screenshot or pixel equality is inferred.

Passing evidence:

- `popup_width::options_cancel_apply_successor_stale_callbacks_and_real_popup_geometry` — linux-full.log:2383
- `popup_width::real_cards_follow_qt_min_max_fixed_gauge_and_wrapped_text_boundaries` — linux-full.log:2448
- `popup_width::pending_eleventh_card_snapshots_on_admission_and_row_removal_keeps_old_policies` — linux-full.log:2369
- `popup_width::legacy_raw_integers_and_independent_edited_fields_are_preserved` — linux-full.log:3030, parity-models.log:1763
- `popup_width::qt_bounds_staging_cancel_loaded_normalisation_and_durable_reopen` — linux-full.log:3039, parity-models.log:1769
- `import::decode::tests::popup_width_imports_raw_reference_preferences_and_reopens_them` — linux-full.log:3929, parity-models.log:2266

Native image inspected:

- /workspace/validation-reviews/ci-ff858ded1/native-renders/popup_width_native.png — SHA-256 `0eba2ad9c1bd1db0fa6308d1d7c7a584d7a23a0b33a08d027ea7531354ec9ad6`

Limits retained:

- Approximate character units only: padded measured native bold sample differs from Qt averageCharWidth, summary bars and platform wrapping.
- Native stack is in MainWindow; separate Qt tool-window, multi-monitor placement, minimized/other-monitor freeze and wider Popup Options parents are outside this slice.
- Existing separately banked API cookie/header leaf is neither credited nor demoted by stale parent-limit wording in this packet.

## namespace-colour-controls

- Eligible: `audit-options-tag-presentation-other-rendering-namespace-for-the-or-top-row`

- Viewed the full fresh950x1600 Options draft. Exact Namespace for the OR top row: label and plain text field show system, readable and fully inside the content width. The neighbouring sibling checkbox is readable; it is outside the selected leaf.
- Correction to prep: the capture shows imported basic system, not untouched None. The measured use ideal tag colour checkbox in the shared test is the sibling namespace control, not the OR field. Selected OR presentation is directly visible; named/missing/empty/new namespace effects are exercised later in the passing live predicate-row colour assertions.
- No Qt PNG was authored by this recorder. Actual Qt OR header and child RGB rows are JSON evidence; native post-Apply OR consumer also lacks a dedicated export. This supports bounded functional/control approval with the stated visual limit, not visual topology or pixel parity.

Passing evidence:

- `namespace_colours::actual_namespace_questions_cancel_retired_owners_reopen_and_live_colours_replay_qt` — linux-full.log:2238
- `namespace_colours::qt_namespace_add_normalization_protected_mixed_delete_and_options_cancel_reopen` — linux-full.log:2955, parity-models.log:1688

Native image inspected:

- /workspace/validation-reviews/ci-ff858ded1/native-renders/namespace-colours-draft.png — SHA-256 `c168fe5cb6e3e23f73325740fb54efb0a460eeb47718f63de29a58dbb373c39f`

Limits retained:

- No Qt PNG exists in this recorder/packet: colour/header/child JSON evidence cannot be described as a Qt visual comparison.
- Native active OR remains one line; expanded header with independently coloured children remains broader Partial.
- Inactive Qt OR connecting-string preference and full namespace colour picker/list menus/keyboard navigation are outside this selected leaf; Options/Tag Presentation parents remain Partial.

## duplicates-progress-label

- Eligible: `audit-options-duplicates-duplicates-filter-page-hide-the-x-done-notification-on-preparation-tab-when-99-searched`

- Viewed the preparation page: preparation label has no percentage, 2,000 eligible files in the system. and Searched1,999/2,000 files at this distance. are readable, with a nearly full gauge. This matches recorded hide=true, distance8, unsearched1/searched1999.
- Qt reference is Options Duplicates, checked selected checkbox; native export is the resulting Main preparation publisher. Surface mismatch is documented and is not an automatic demotion. The native selected checkbox itself is driven/asserted in the green GUI test but has no dedicated Options PNG.
- An unrelated auto-resolution tab caption is truncated at the sidebar edge; it does not obscure the selected preparation label/counts and does not establish wider sidebar parity.

Passing evidence:

- `duplicates_progress::actual_checkbox_relabels_visible_preparation_without_changing_work_and_future_pages_reopen_policy` — linux-full.log:2030
- `duplicates_progress::qt_staging_persistence_and_scoped_save_drive_strict_caught_up_preparation_labels` — linux-full.log:2796, parity-models.log:1529
- `duplicates_progress::tests::real_saved_disabled_legacy_policy_backfills_native_override_and_reopens` — linux-full.log:3872, parity-models.log:2209

Native image inspected:

- /workspace/validation-reviews/ci-ff858ded1/native-renders/duplicates-progress-policy-native.png — SHA-256 `60bf725f987bcc70e41b4296497c4bc077e505c6e9afec54f2551b315c6fc389`

Limits retained:

- Qt Options PNG and native Main publisher PNG show different surfaces/states; compare labels and recorded effects within scope, without pixel-parity assertions.
- Preparation maintenance, duplicate colours, background search workers and Options parent completion remain outside leaf. Existing publication path is reused, with no added observer/timer.

## quick-export-directory

- Eligible: `audit-options-menu-menu-file-quick-export-directory`

- Viewed fresh File/open cascade: installation directory, database directory and quick export directory appear in recorded order, readable and enabled. Qt reference is the tight open-submenu crop with these same labels/order.
- Native submenu is drawn at the upper-left over menu chrome; this capture is generated by direct menu_line_clicked arguments0,0,0 rather than actual pointer row geometry. It is evidence for the live menu contents/action, not physical submenu placement parity or a proven production positioning defect. The existing launcher is substituted to record targets; no real OS file manager launch is claimed.

Passing evidence:

- `quick_export_directory::actual_file_menu_replays_saved_paths_fallback_errors_cancel_and_reopen` — linux-full.log:2400
- `quick_export_directory::hidden_rebound_closed_and_reentrant_resolver_owners_cannot_launch_or_create` — linux-full.log:2401
- `quick_export_directory::actual_file_submenu_enables_and_selects_quick_export_directory` — linux-full.log:3037, parity-models.log:1771
- `quick_export_directory::tests::explicit_empty_home_wins_and_account_lookup_only_runs_for_absent_home` — linux-full.log:1868
- `import::decode::tests::export_default_directory_import_resolves_recorded_portable_paths` — linux-full.log:3907, parity-models.log:2244

Native image inspected:

- /workspace/validation-reviews/ci-ff858ded1/native-renders/quick-export-directory-native.png — SHA-256 `6a7d9a9e30e1fa972a57c8e511f8e98df7d1463116871eac281e830172fbcb1e`

Limits retained:

- Qt recording and required validation are Linux; platform target normalization is documented while raw saved strings remain unchanged. Windows/macOS success not inferred.
- Launcher arguments are tested with existing launcher substituted; real OS file manager behavior and selection are outside leaf.
- No path picker/confirmation or broad File/database menu completion follows from this leaf.

## Reference images actually inspected

- /workspace/hydrus-mine/oracle/fixtures/popup_width_reference.png — SHA-256 `8949046cc954f7beff68fa54cd2e00a24d0a402d814deb10fa9261a64efc6e62`
- /workspace/hydrus-mine/oracle/fixtures/duplicates_progress_option_reference.png — SHA-256 `a9a9cf1e8dbfbddea56f355e2bb317ef5a7444a90b0cfd3854344ba9fa2e1c56`
- /workspace/hydrus-mine/oracle/fixtures/quick_export_directory_reference.png — SHA-256 `68bcb7009f0049e6220b3d9c1d2da92822c807f4ce22bb48ba7b41d2d2742a42`

Eligible to replace stale authored/pending-hosted-validation wording for these five finite leaves after root assembles the complete exact-source publication gate. Preserve all semantic, parent, visual, timing and platform limits.

Scratch reports only. No canonical edits, builds or local test execution. No blocking defect found within the five finite leaves.
