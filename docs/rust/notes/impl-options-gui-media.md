# Options pages (options-gui, options-media): report

No Qt/Python reference environment exists in this sandbox (`~/pyenv` is missing,
no libmpv), so nothing here was recorded. Tests replay the reference's source
(formulae, labels, defaults) and, for the page layouts, the existing
`options_dialog.json` recording.

## Leaves tagged

| Leaf | Test |
|---|---|
| audit-options-nested-frame-location-gravity | `tests/model/frame_placement.rs` (2), `options_gui_frames::default_gravity_and_position_...` |
| audit-options-ratings-preview-window-preview-window-like-dislike-and-numerical-rating-icon-size | `preview_top_right::the_preview_window_draws_its_ratings_at_the_sizes_the_options_say` |
| audit-options-ratings-preview-window-preview-window-inc-dec-rating-icon-height | same |
| audit-options-media-viewer-hovers-preview-window-hovers-draw-ratings-and-locations-top-right-in-preview-window-background | `preview_top_right::the_top_right_hover_draws_in_the_background_and_pops_in_...` |
| audit-options-media-viewer-hovers-preview-window-hovers-pop-in-this-hover-on-mouseover | same |
| audit-options-media-viewer-hovers-hover-windows-pin-the-duplicates-... | `duplicate_hover_pin::the_duplicates_hover_is_pinned_or_pops_in_...` |
| audit-options-media-playback-mpv-set-a-new-mpv-conf-on-dialog-ok | `options_media_mpv::a_new_mpv_conf_replaces_the_databases_...` |
| audit-options-media-playback-mpv-preferred-audio-output-device, ...-debug-loop-playlist-instead-of-loop-file-in-mpv, ...-debug-set-null-audio-device-on-silent-media | `options_media_mpv::the_mpv_rows_set_what_a_player_is_told_...` (libmpv absent: it drives `mpv_options::Plan`, the exact `set` commands the player executes, not mpv) |
| audit-options-ratings-choose-rating-service-style-...-select-rating-service-for-styling-numerical-stars | `options_gui_ratings_examples::the_chosen_service_styles_the_example_stars_...` |

## Implemented

- `frame_placement` (model) + `windows::placement`: default gravity and
  top-left / centre / mouse positions, slide-back, display limit.
- Preview pane top-right hover (`preview_hover.slint`, `preview_window.rs`).
- Duplicate filter hover pin (`duplicate_filter.slint`, `DuplicatePanel`).
- Tag suggestions page: reference notebook pages (`Item::Tab`), show related,
  durations, concurrence, recent count, file lookup switch.
- mpv.conf row, `mpv_options::Plan`, null audio for silent media.
- Ratings page: style dropdown and the four example rows.
- Model test replay now descends into the recording's notebook tabs.

## Not tagged although touched

- `audit-options-nested-tag-suggestions-tabs`: related (show, concurrence), recent
  (count) and most used work; the related durations and the file lookup tab have no
  consumer (exact ranking; no lookup scripts), and the quick-dialog count has no row.
- `audit-options-gui-frame-locations-edit`: gravity/position now reach the main
  window, media viewer and Options window only; other dialogs don't consult frames.
- `audit-options-options-search`: the reference also searches descriptive labels
  and every widget text; unchanged.

## Not applicable / not done, with reasons

- audio "preview window has its own volume": the preview shows a still and plays
  nothing, so nothing reads it.
- Prefer system FFMPEG: hydrus-rs has no bundled ffmpeg; it always uses `PATH`.
- Self-sizing media viewer rescue padding: no self-sizing viewer.
- Freeze toaster on another display: popups are inside the main window.
- Petition reasons count: no tag repositories to petition.
- Integer locale: needs CLDR number grouping and unverifiable locale data; left
  stored only.
- "Fetch list of mpv audio device strings": needs a running libmpv (absent here)
  and a new button row kind.
- Application display name on secondary windows (partial leaf): each Slint window
  has its own title; no central place.
- Idle autosave partial leaf: reading the pointer outside the app's windows is
  not portable in winit.

## Workflow friction

- Fresh machine: first `dev.sh ui` ~11 min (dependencies already warm-ish),
  `dev.sh slint` needs a 3 min build of its checker the first time, first model
  test build 3m16s. Each `.slint` change then costs ~5.5 min of `hydrus-gui-ui`
  plus ~1 min for the GUI test crate; Rust-only loops are 10-40 s.
- `git stash`/`pop` touched every file's mtime and forced a needless UI rebuild.
- `dev.sh gui a b` takes only the first filter.
- Row kind numbers in `options.slint` are bare integers; I collided with 38
  (media views table) and the test, written against my own number, passed
  until I looked at a render.
- The new tests found `Window::size()` is 0 before layout in the headless
  platform, so one-axis gravity can only be tested through `windows::placement`.
- `emoji_fonts::outline_fox_is_selected_without_replacing_platform_text_fallbacks`
  fails in this sandbox (font environment); it doesn't touch anything changed here.

## Final section (after the coordinator's change of plan)

Finished since the last section:
- Review findings 1-4 on the first batch (style key kept at once and shape/colours
  only; background vs popped-in preview sizes, inc/dec width and draw order with
  tests on the drawn rows; `has_audio` test; frame padding, pointer position,
  fullscreen parent and placing the Options window before it shows).
- Merged `origin/claude/pensive-darwin-kvjhpq` and ran `scripts/setup-oracle.sh`.
  With the reference now runnable, two source-read tests became recordings:
  `oracle/record_frame_placement.py` -> `fixtures/frame_placement.json` (11 frame
  settings; the Rust formulae match all of them on the first run, which also
  tags `audit-options-geometry` with the gravity leaf) and
  `oracle/dump_incdec_sizes.py` -> `fixtures/incdec_sizes.json` (replaces the
  hand-worked inc/dec width test).

Started and dropped (nothing half-built is on the branch):
- Application display name on secondary windows: Slint windows own their title
  properties and there is no central hook (winit's window attributes hook runs
  before the title is known). Not started in code.
- The ratings-example template behaviour and preview hover sizes beyond the
  inc/dec width were not re-recorded (the reference needs a controller to read
  its options; ~half a day).

Remains, with rough sizes:
- `audit-options-gui-frame-locations-edit`: other dialogs consulting their frames
  (~12 windows, one line each plus save on close; about a day).
- `audit-options-options-search`: descriptive labels and widget texts (a day).
- Tag suggestion tabs remainder: no consumer can exist for file lookup scripts or
  quick-dialog counts; related durations need a time-sliced search (a day).
- `shell` workstream (popups modal/download/freeze, status activity, about
  libraries) and `audit-options-menu-menu-file-options`,
  `audit-network-export-examples`: not started (several days).
