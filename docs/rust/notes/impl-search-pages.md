# Search pages: report

## Leaves tagged (14 of 16)

| Leaf | Test |
|---|---|
| audit-options-search-active-edit | `active_predicates::` existing-size edit, captured-menu and inherited-menus tests (recordings `active_predicate_*.json`) |
| audit-options-weight-detail | `search_pages_menu::total_session_weight_explains_the_number_...` (drives Pages > weight; open and closed file/URL weights) |
| audit-options-search-star-save | `favourite_search_editor::save_this_search_opens_the_captured_search_...` (star menu, captured values, edit name/domain/sort/collect, manager Apply) |
| audit-options-favourites-edit-predicates | `favourites::the_dialogs_save_edit_and_delete_searches` (typing, parse error, double-click removal) |
| audit-options-predicate-custom-defaults | `predicate_editors::predicate_star_save_and_reset_...` and `imported_predicate_defaults_...` (recorded menus, reopen) |
| audit-options-predicate-rating-{ratinglike,ratingnumerical,ratingincdec}-service | `predicate_editors::each_rating_panel_is_for_its_own_service` |
| audit-options-predicate-time-{archived,modified}-...-date-time | `predicate_editors::archived_and_modified_date_panels_make_...` |
| audit-options-menu-menu-pages-new-{gallery,simple-downloader,watcher}-page, new-duplicates-processing-page | `search_pages_menu::the_download_and_special_menus_make_each_kind_of_page_...` (names, kinds, persistence) |
| audit-options-menu-menu-pages-all-multiwatcher-highlights | `search_pages_menu::clear_all_multiwatcher_highlights_clears_every_watcher_page` |

Most of this was already implemented; the states were stale. No production code changed.

## Judgement calls

- Rating like/numerical/inc-dec "service selection": the reference has no selector in these panels (each is built for one service and shows its name); only the advanced panel has a chooser. I tagged the existing per-service test.
- Date-time leaves: the recorder only ran import and last-viewed; archived and modified share the reference's base class, so the test replays those scenarios with their own kind. Dates are typed fields, not a calendar (in DIFFERENCES).
- Active-edit: the active list's inherited sibling/parent entries are not covered (tracked with tag relationships).
- Multiwatcher test covers watcher pages only (first watcher auto-highlighted); gallery pages were not checked as untouched.

## Not done in the first pass

- similartodata-paste (done in the final section below).

## Friction

- First `dev.sh ui` build: 12 min here (737 s), then ~1 min per test-binary relink.
- A missing `AdvancedMode(true)` hides domain choices in the favourite editor; the existing test sets it without a comment.
- Highlight rules (first watcher auto-highlighted, activating the shown one clears it) cost one failed run.

## Final section (after the coordinator's change of plan)

Finished: `audit-options-predicate-similar-files-data-similartodata-paste`.
"Paste image!" now takes the clipboard bitmap (arboard `image-data`, hakari
regenerated and verified, `Cargo.lock` updated), else a file path, else text,
in the reference's order; the bitmap is written as a PNG and hashed like a
file. `predicate_editors::paste_image_takes_a_clipboard_bitmap_...` checks
nothing/path/bitmap/duplicate/invalid-path and "clear". Not compared against a
recording of the reference's bitmap path (no oracle run; the merge of the
base branch and `scripts/setup-oracle.sh` were not done).

Started and dropped: nothing in code. The merge of
`origin/claude/pensive-darwin-kvjhpq` was refused by the permission
classifier, then the plan changed.

Not started: the `editors` workstream (Slint tooltips unexamined: I did not
check whether Slint 1.18 has a tooltip mechanism) and the `network` leaves
(`audit-network-exchange-unsupported`, `exchange-domain`,
`import-external-program-entry`). Sizes unknown; I read none of their code.

Note: the archived/modified date-time tags rest on replaying the import and
last-viewed scenarios; the coordinator is re-recording them.

Friction: `cargo-hakari` was not installed (~4 min to install); the
`image-data` change rebuilt hydrus-gui in ~7 min.
