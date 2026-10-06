Both selected concrete IDs qualify for bounded Linux sign-off at `7c3c1aac5a60a9e3fb1e7a8864cb439914d4fd60`. Required full Linux run `37447108755` completed successfully: check `112214498612` and parity-models `112214498972`, strict Clippy, 697 GUI tests and 67 media unit tests passed with no failures. Fresh artifact `11406361462` has ZIP SHA-256 `71a8b279207b3d5bfabe1cba076b684a3999908e5cf659993df1fc1e6ee338e5`. No source/behavior/fresh visual blocker was found.

## audit-options-media-playback-system-apply-image-icc-profile-colour-adjustments

Approval: **true**, for the finite behavior below.

Real enabled-default Media Playback checkbox stages/cancels/applies, imports encoded ClientOptions or retained legacy stores, and reopens saved policy. Embedded ICC can be ignored without disabling independent PNG gamma/chromaticity fallback. Existing importers/maintenance and owned preview/viewer/archive/duplicate raster consumers use policy; accepted static pixels/tiles refresh without resetting zoom/focus/viewing intervals. Immutable admission policy and generations reject obsolete/retired replies, including held conversion across same-window rebinding. Paused native animations retain accepted frame/index and read saved policy for future conversions; DoNotShow preview remains prohibited after invalidation.

Actually viewed `/workspace/validation-reviews/ci-7c3c1aac5/native-renders/icc-viewer-policy-native.png` (800×600), SHA-256 `1b9f6031b788252515b119098d3b2ab392847e9bbfc5e10c92c8f5c0578b681f`. Actual 800x600 native viewer displays a smooth magenta/green gradient at retained7500% zoom with side margins and owned viewer tags/status. Capture follows saved ICC disabled Apply and exact ignored-profile main/preview/sharp-tile byte assertions. No blank raster or bounded clipping defect observed. Qt Options image is a different surface; no paired bitmap parity.

Passing exact tests:

- `crates/hydrus-store/src/image_colour.rs::tests::real_saved_disabled_legacy_policy_backfills_native_override_and_reopens` — [linux-full.log:3892](/workspace/validation-reviews/ci-7c3c1aac5/linux-full.log:3892).
- `crates/hydrus-gui-model/tests/model/image_colour.rs::qt_icc_checkbox_cancel_saved_reopen_preserves_concurrent_viewer_rules` — [linux-full.log:2900](/workspace/validation-reviews/ci-7c3c1aac5/linux-full.log:2900).
- `crates/hydrus-media/tests/image_colour.rs::embedded_policy_preserves_gamma_fallback_and_real_png_jpeg_webp_pixels` — [linux-full.log:3590](/workspace/validation-reviews/ci-7c3c1aac5/linux-full.log:3590).
- `crates/hydrus-media/tests/image_colour.rs::native_owned_animation_policy_changes_future_frames_without_resetting_position` — [linux-full.log:3589](/workspace/validation-reviews/ci-7c3c1aac5/linux-full.log:3589).
- `crates/hydrus-import/tests/image_colour.rs::actual_import_pixels_and_generated_thumbnails_follow_saved_policy_without_recreating_importer` — [linux-full.log:3274](/workspace/validation-reviews/ci-7c3c1aac5/linux-full.log:3274).
- `crates/hydrus-gui/tests/gui/image_colour.rs::actual_saved_icc_updates_preview_viewer_tiles_and_archive_without_resetting_owned_state` — [linux-full.log:2138](/workspace/validation-reviews/ci-7c3c1aac5/linux-full.log:2138).
- `crates/hydrus-gui/tests/gui/image_colour.rs::held_old_colour_reply_cannot_replace_current_or_rebound_canvas_and_ineligible_preview_stays_empty` — [linux-full.log:2140](/workspace/validation-reviews/ci-7c3c1aac5/linux-full.log:2140).
- `crates/hydrus-gui/tests/gui/image_colour.rs::paused_animation_keeps_accepted_frame_index_and_pixels_on_icc_notification` — [linux-full.log:2136](/workspace/validation-reviews/ci-7c3c1aac5/linux-full.log:2136).
- `crates/hydrus-media/tests/image_colour.rs::alpha_bearing_animation_retains_existing_blend_disposal_pixels_and_durations` — [linux-full.log:3588](/workspace/validation-reviews/ci-7c3c1aac5/linux-full.log:3588).

Preserved limits:

- Finite switch/consumer scope does not establish universal format/plugin/mpv renderer parity or add the separate browser eye-menu ICC action. Broader media rendering/playback parents remain Partial.
- Existing durable pixel hashes/profile-free stored thumbnails are not regenerated on Apply, matching the reference cache publications. Explicit maintenance regeneration and future imports use saved policy.
- Static image refresh preserves intervals/zoom. Paused animation frames are retained; future frame conversions consume live policy, without promising prefetched-frame cancellation that the reference image-cache notification does not provide.
- Qt Options and native viewer are different surfaces/states; no paired bitmap or dedicated native checkbox image.
- Separate truncated loading remains unimplemented and earns zero credit; browser eye-menu ICC, universal formats/plugins/mpv and Media Playback parents are outside scope.
- 100ms owner-local watch is native polling/cache notification integration, without platform scheduling/render/font parity.
- Validated Linux only at exact7c source; Windows/macOS deferred. No parent-family, alias or unrepresented renderer/control credit.

## audit-options-media-playback-zoom-and-position-preview-viewer-default-zoom

Approval: **true**, for the finite behavior below.

Saved Preview Viewer default zoom control offers the actual six Qt choices with Default for filetype default, independently from full viewer zoom. Typed native values win over retained ClientOptions; import and old-store fallback retain the preview override. Staged Apply/Cancel/reopen and untouched concurrent preview edits are covered. Accepted still/poster previews use preview-specific filetype scale rules, exact integer/DPR-aware centered canvas bounds and clipping. New media acceptance and actual viewport/DPR changes read the saved policy; saving alone and ICC/raster-cache presentation preserve current geometry. Existing admission, active/current owner checks, DoNotShow, decoder request retirement and viewing interval boundaries remain unchanged.

Actually viewed `/workspace/validation-reviews/ci-7c3c1aac5/native-renders/preview-default-zoom-fill-native.png` (1400×1000), SHA-256 `221e38e10fcc47318b40dd44730b41ab5b2e86229714f8a9a7eec3f6ef4e8cf0`. Actual 1400x1000 native main window: flatRGB43 fills the entire lower-left short preview pane to its clip; thumbnail remains the colourful source image. This is deliberate injected120x80 geometry evidence. Source asserts projected(0,-31,360,240) and actual painted pixel bounds, rather than comparing the synthetic RGB bitmap against Qt.

Actually viewed `/workspace/validation-reviews/ci-7c3c1aac5/native-renders/preview-default-zoom-native.png` (1400×1000), SHA-256 `9c9649e556dfdea1f371b8b5cf5cf9cf96b8fb753d1b5290e22b66e11fc1e08b`. Actual 1400x1000 native main window: flatRGB43 appears as a120x80 rectangle centered within the360x240 lower-left preview, with visible margins on all sides. Source asserts(120,80,120,80) after actual resize samples saved100%, whereas save-only and ICC replacement preserve prior FillX geometry. No selected-scope visual blocker.

Passing exact tests:

- `crates/hydrus-gui-model/tests/model/preview_default_zoom.rs::actual_qt_choices_staged_cancel_save_reopen_and_independent_viewer_policy` — [linux-full.log:3037](/workspace/validation-reviews/ci-7c3c1aac5/linux-full.log:3037).
- `crates/hydrus-gui-model/tests/model/preview_default_zoom.rs::exact_real_preview_geometry_uses_preview_rules_and_keeps_full_viewer_zooms` — [linux-full.log:3036](/workspace/validation-reviews/ci-7c3c1aac5/linux-full.log:3036).
- `crates/hydrus-gui-model/tests/model/preview_default_zoom.rs::retained_legacy_fallback_and_native_wins_survive_store_reopen` — [linux-full.log:3041](/workspace/validation-reviews/ci-7c3c1aac5/linux-full.log:3041).
- `crates/hydrus-gui/tests/gui/preview_default_zoom.rs::real_options_cancel_reopen_six_modes_paint_clipped_geometry_and_preserve_current_on_save` — [linux-full.log:2396](/workspace/validation-reviews/ci-7c3c1aac5/linux-full.log:2396).
- `crates/hydrus-gui/tests/gui/preview_default_zoom.rs::held_raster_acceptance_samples_saved_preview_policy_without_restart_or_prohibited_admission` — [linux-full.log:2384](/workspace/validation-reviews/ci-7c3c1aac5/linux-full.log:2384).

Preserved limits:

- Only this original concrete control is proposed; media-playback/zoom parents, aliases and interactive preview zoom/pan remain Partial with zero credit.
- Backend consumer scope is accepted static/poster preview raster geometry. This does not add embedded media or animation/video players, nor alter their admission.
- 48 actual Qt MediaContainer SetMedia rectangles and48 actual Qt DPR2 calculated sizes cover six choices/four resolutions/two panes; DPR2 cases are backend arithmetic evidence, not an actual HiDPI native screenshot.
- The existing basic JPEG metadata reports120x80 while its decoded payload is64x48; recorded canvas geometry uses existing metadata. The Qt warning is retained in the recording log, and pixel-identical bitmap parity is not claimed.
- Actual Qt parent resize resets the default policy; native implementation samples on accepted media or actual measured viewport/DPR changes while preserving ICC/cache redisplay and unrelated full-viewer zoom state.
- FlatRGB43 is intentional injected geometry evidence, not failed decode or bitmap parity against colourful Qt screenshot.
- No dedicated native Options dropdown capture; native tests assert exact choices/reopened indices, while Qt screenshot provides control presentation.
- Validated Linux only at exact7c source; Windows/macOS deferred. No parent-family, alias or unrepresented renderer/control credit.

## Actual Qt reference inspection

- `/workspace/hydrus-mine/oracle/fixtures/image_decoder_policies_reference.png` — SHA-256 `ea056c94eaf7aac4d1106152dda008c8de97feae30291c61e44e5907370d303e`; actually viewed again in this final review and pinned to exact7c Git bytes.
- `/workspace/hydrus-mine/oracle/fixtures/preview_default_zoom_qt.png` — SHA-256 `30cc5f6112040555d1bad373d68c9617dfc90cbd293b26e2e28fcd6ef4ca7818`; actually viewed again in this final review and pinned to exact7c Git bytes.
- `/workspace/hydrus-mine/oracle/fixtures/preview_default_zoom_canvas_qt.png` — SHA-256 `72547402217dee1e6d63bd3904f1f7c141b8b9e3749f7f899c523f089ee3d97b`; actually viewed again in this final review and pinned to exact7c Git bytes.

The ICC Qt reference contains two stacked Options states (upper unchecked, final lower checked); the native export is the consumer viewer. The Qt preview dropdown is readable with default-for-filetype selected; an unrelated compressed mpv section is outside the selected control. Qt’s colourful real canvas differs from the deliberate native flatRGB geometry captures. Different surfaces/states do not establish paired pixel parity.

All scoped source/test/reference hashes and exact passing log lines are bound in the companion JSON. Original accepted ownership/admission boundaries remain: hidden/retired/rebound Options cannot save, held old conversions cannot replace successor pixels or add durable views, paused animations retain accepted frame/index, DoNotShow stays empty, and preview request timing/projection survive ICC redisplay. DPR2 evidence is arithmetic, not a native HiDPI screenshot.

This report grants only the two listed original concrete leaf IDs; broader parents, aliases, interactive preview zoom/pan, stored-thumbnail regeneration, truncated loading, browser eye-menu ICC, universal formats/plugins/mpv and deferred Windows/macOS gain no credit. Scratch files only; no builds/product edits. The RGB-picker proposal stays unapplied.
