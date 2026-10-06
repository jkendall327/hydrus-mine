# Final four-leaf Linux review: 6b66240c16164a0021f0dd9e128995a25ed28c1d

All four selected IDs are eligible for bounded Linux sign-off. Required check/parity-models jobs in run37438105898 succeeded. Exact scoped tests pass in saved logs; GUI696pass/0fail/0ignored and media67pass/0fail/0ignored. All scoped source/reference hashes match this exact source.

Fresh artifact11401641954 has zip SHA-256 `bc98decc6ffa86b0cedd7df33899202bb83711bcd2b9e4a40c72892baddd5c67` matching upload log. Four fresh PNGs were actually viewed and their hashes/byte counts/dimensions verified against its manifest. No Qt PNG exists in these recorders: functional Qt JSON/source evidence is distinct from visual comparison.

Exactly four selected leaves approved; clear-all excluded,280banked preserved, no parent/alias credit, Windows/macOS deferred.

## viewer-drag-anchor

- Approved: `audit-options-media-viewer-mouse-behaviour-anchor-mouse-cursor-during-media-viewer-drags`
- Approved: `audit-options-media-viewer-mouse-behaviour-if-set-to-anchor-drags-undo-on-apparent-touchscreen-drag`

- options_viewer_drag_anchor.png: selected Media Viewer page displays exact anchor and apparent touchscreen labels, both checked. Their captions and checkmarks fit inside the1100x850 viewport. Apply and Cancel are readable. Unselected seek-bar optional caption truncates at the right; that does not obscure these selected controls and gives no broader Options credit.
- viewer_drag_anchor.png: actual JPEG remains drawn at the panned position after recorded physical pointer motions. The image extends beyond the viewport; this is the intentional pan/large existing zoom scene, not evidence of a zero-sized/missing image. Still pixels cannot prove cursor warp execution or numerical drag deltas. Those remain separately source/reference/scoped-test evidence.

Fresh images actually viewed:

- /workspace/validation-reviews/ci-6b66240c1/native-renders/viewer_drag_anchor.png — SHA-256 `b519180a40cbe0de985cba2abe5dd07bcf81e701612e73a0a8c92bdabbdd912a`
- /workspace/validation-reviews/ci-6b66240c1/native-renders/options_viewer_drag_anchor.png — SHA-256 `704cd177c66278a648a1c6e6851d01916dfdcaf5a3d42497d4c2f8adff0bd414`

Passing exact tests:

- `viewer_drag::anchored_and_touch_overridden_drags_replay_actual_qt_threshold_and_warps` — linux-full.log:3191, parity-models.log:1930
- `viewer_drag::anchor_options_remain_drafts_until_apply_and_survive_reopen` — linux-full.log:3193, parity-models.log:1932
- `viewer_drag::actual_pointer_drags_replay_qt_and_options_refresh_cancel_and_reopen` — linux-full.log:2608
- `viewer_drag::two_visible_viewers_read_saved_preferences_and_keep_independent_live_lifetimes` — linux-full.log:2611
- `import::decode::tests::viewer_pointer_options_import_both_drag_preferences` — linux-full.log:3942, parity-models.log:2285

Caveats retained:

- No Qt PNG: this recorder writes JSON only. Fresh native controls/scene images were actually inspected; this cannot be called a paired Qt visual comparison.
- Physical cursor warp requires OS/compositor support; native requests through existing Winit window may fail/are ignored in headless backend. Pure replay proves request semantics; GUI proves actual pointer-to-media deltas, not compositor execution.
- No full touchscreen-event support, preview/filter canvases, other mouse controls or broader Media Viewer parent credit.
- Still PNG records the intended panned image beyond viewport; it does not prove numerical motion or compositor warp. Exact requested warps/state come from reference/model replay and actual media deltas from passing physical pointer GUI test.
- Unselected seek-bar optional caption is truncated at the right in Options capture; selected anchor/touch labels and checkmarks are fully readable. No broader Options parity follows.
- Linux-only required validation; Windows/macOS deferred and not approved. No parents or aliases earn completion credit.

## viewer-tag-wheel

- Approved: `audit-options-media-viewer-hovers-hover-windows-allow-a-mouse-wheel-scroll-over-the-taglist-to-propagate-to-the-main-canvas`

- viewer-tags-wheel.png: left hover is visible with readable wheel:000 and following rows, scroll bar and thumb. Rows clipped at top/bottom are the scrolled clipped list viewport expected after one real wheel event, not vanished tag content. The JPEG remains shown; caption/navigation unchanged is established by the exact GUI assertion, not screenshot equality.
- The native capture is a1000x300 short viewer after first list consumption under policynever. It does not capture the four-choice Options control, edge navigation, timing boundaries or Ctrl zoom. These remain recorded/runtime assertions with no separate native PNG.

Fresh images actually viewed:

- /workspace/validation-reviews/ci-6b66240c1/native-renders/viewer-tags-wheel.png — SHA-256 `ecc16f05182d385b79cbdc367f65335439fe81907177d5c7eeb45286fc15586a`

Passing exact tests:

- `viewer_tag_wheel::parent_wheel_gates_replay_actual_qt_boundaries_and_directions` — linux-full.log:3198, parity-models.log:1937
- `viewer_tag_wheel::four_policy_choices_stage_cancel_save_and_reopen_with_reference_labels` — linux-full.log:3197, parity-models.log:1938
- `viewer_tag_wheel::long_tag_hover_scrolls_before_policy_gated_real_navigation_and_zoom` — linux-full.log:2627
- `viewer_drag::two_visible_viewers_read_saved_preferences_and_keep_independent_live_lifetimes` — linux-full.log:2611
- `import::decode::tests::viewer_tag_wheel_imports_all_four_reference_policy_codes` — linux-full.log:3945, parity-models.log:2288

Caveats retained:

- No Qt PNG: recorder writes JSON only. Fresh native hover PNG proves presentation/scroll state but cannot visually compare Qt pixels.
- Measured native row heights and Winit line normalization(60logical pixels per line; three rows per tick) differ from Qt pixels/platform line preferences; fractional Qt wheel accumulation is not claimed.
- Ordinary browser viewer hover only. Preview/filter canvases without native hover panes, full tag selection/context actions and broader hover/list parents remain Partial or Missing.
- Only viewer-tags-wheel.png is exported: selected Options dropdown and subsequent navigation/zoom/timing states are asserted but have no dedicated native captures.
- Strict0.57-second delay/direction/new-media grace and250-second reset are matched under recorded/model clocks. Native physical wheel test uses650ms quiet waits; OS-level event timing equality is not claimed.
- Top/bottom list rows are clipped by the scrolled viewport as intended; image caption unchanged is established by the passing test, not a paired before/after screenshot.
- Linux-only required validation; Windows/macOS deferred and not approved. No parents or aliases earn completion credit.

## viewing-statistics-cleanup

- Approved: `audit-media-menu-database-clear-cull-file-viewing-statistics-based-on-current-min-max-values`

- viewing_statistics_cull_question.png: all four explanation paragraphs are fully readable, including minimum100views/100seconds ->50views, maximum10views/100000seconds ->600seconds, and separate preview/media rules. do it and forget it buttons are fully inside680x420. They are tall in this native layout; no selected question text/button clipping observed.
- Headless capture omits operating-system titlebar; Are you sure? is set by source rather than visible in this PNG. Acceptance/completion/error states are runtime asserted and unexported. The native question is still pending and no rows have been culled at capture.

Fresh images actually viewed:

- /workspace/validation-reviews/ci-6b66240c1/native-renders/viewing_statistics_cull_question.png — SHA-256 `685722ccc13f502dee0e319a3de37b9fbc9e128f382fabb2876217624da0e86c`

Passing exact tests:

- `viewing_maintenance::reference_clear_cull_questions_rules_errors_and_real_reopened_media_match` — linux-full.log:3205, parity-models.log:1944
- `viewing_maintenance::cull_reads_current_rules_and_a_preview_validation_failure_preserves_media_rows` — linux-full.log:3200, parity-models.log:1939
- `viewing_maintenance::real_menu_clear_and_cull_read_live_rules_preserve_declines_and_reopen` — linux-full.log:2620
- `viewing_maintenance::retired_hidden_and_successor_owners_cannot_clear_or_cull_current_records` — linux-full.log:2621
- `viewing_maintenance::closing_and_dropping_the_question_releases_its_timer_owner` — linux-full.log:2619

Caveats retained:

- No Qt PNG: recorder captures scripted question/Information strings and real DB results as JSON, not a photographed QMessageBox. Fresh question PNG was viewed: all paragraphs and both buttons are readable; no paired Qt screenshot exists.
- Native invalid-rule errors use owned Warning notice rather than reference global Python exception presentation; values exceeding native SQL integer range are rejected atomically.
- Preview min/max Options rules support culling only; missing timed preview-display consumer remains unclaimed, zero extra credit.
- Broader database maintenance, viewing-statistics parents/aliases and unselected clear-all remain unresolved; no credit from shared tests or packet.
- Completion and invalid-error notice states are asserted but not exported as dedicated PNGs.
- Native question buttons are tall but labels and all paragraphs fit. Headless PNG has no OS titlebar; Are you sure? is source metadata, not visible-decoration parity.
- Timer owner/visibility and synchronous one-shot acceptance remain bounded to this owned question; the existing250ms invalidation timer does not establish broad timing or modal-platform parity.
- Linux-only required validation; Windows/macOS deferred and not approved. No parents or aliases earn completion credit.

Only stale hosted-tests/native-render-inspection-pending wording for these four finite selected leaves is eligible for removal after root assembles final21 publication gate. Preserve all compositor, clock, row geometry, missing exports, exception presentation, platform and unselected-parent limitations.

Scratch report only; no builds/local tests, Qt recorder runs, product/canonical edits or claims for clear-all.
