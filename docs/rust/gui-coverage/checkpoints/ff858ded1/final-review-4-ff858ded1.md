# Final scoped Linux review: ff858ded1

**All four assigned original leaves are eligible for Linux first-pass sign-off**, retaining their manifest limits. Exact source `ff858ded14518d4a9a1d39bb5aafe0213c992983`, successful run `37428534955`, required jobs `112153683030`/`112153683270`; 696 GUI passed, 0 failed, 0 ignored. Windows/macOS remain deferred. No parent credit, exact-pixel parity, canonical edits or publication.

Artifact `11396734519`, ZIP SHA-256 `baa000def90c94892745407a1204dc868833361f3ebd7e32b941f2f1a2ab594f`. Five fresh native images were actually viewed and their hashes matched the artifact manifest; two Qt images were actually viewed. Exact source/test/reference hashes from preparation were rechecked against the frozen commit.

## audit-options-gui-pages-navigation-and-drag-and-drop-experimental-show-tab-tree-view

**Approve scoped Linux leaf.** Recorded defining actions/settings reach actual native consumers; staged Apply/Cancel/reopen and applicable owner boundaries pass. Fresh available surfaces are usable within declared framework/fixture limitations; missing captures do not establish visual parity for uncaptured surfaces.

`notebook_tree::real_tree_mouse_and_keys_replay_cursor_activation_and_preserved_child_expansion` passed at `linux-full.log:2278`. `tab_presentation::apply_cancel_reopen_and_four_sides_preserve_real_nested_selection_and_full_names` passed at `linux-full.log:2558`. `tab_presentation::recorded_hide_gate_retains_live_hierarchy_access_and_elision_changes_actual_paint` passed at `linux-full.log:2582`. `page_tree::cursor_expansion_and_activation_replay_actual_frame_tree` passed at `parity-models.log:1744`. `page_tree::stable_keys_survive_reorder_rename_and_closed_cursor_without_stale_activation` passed at `parity-models.log:1745`. `tab_presentation::legacy_defaults_and_staged_controls_replay_actual_options_then_reopen` passed at `parity-models.log:1888`.

Caveats:

- Qt capture is a 252x724 tree-only widget; native capture is a 900x600 Main window with 190px sidebar/26px rows/text toolbar. Compare hierarchy, disclosure and cursor states; no exact-pixel parity.
- Native double-click steps replay selected/toggled/chosen callbacks; Qt uses physical double clicks. Native single-click/disclosure and arrow/Home/End/Return are real events.
- Drag/drop, context menus, filters/depth/history/cog/empty-space creation/optional collapsed descendants/broader keys remain outside this leaf. Cursor/expansion are live owner-local state, not persisted session content.
- No native PNG of the Options tree selector was found in its assigned tests; functional staged-choice assertions cover it. Supplemental tab PNGs do not substitute for a selector capture.

## audit-media-notes-missing-cog

**Approve scoped Linux leaf.** Recorded defining actions/settings reach actual native consumers; staged Apply/Cancel/reopen and applicable owner boundaries pass. Fresh available surfaces are usable within declared framework/fixture limitations; missing captures do not establish visual parity for uncaptured surfaces.

`manage_notes::notes_are_added_edited_and_deleted_as_the_reference_does` passed at `linux-full.log:2200`. `notes_preferences::options_cog_cursor_copy_and_live_hover_replay_with_owned_cancel_and_reopen` passed at `linux-full.log:2271`. `import::decode::tests::four_note_preferences_import_and_survive_reopen` passed at `linux-full.log:3911`. `notes_preferences::actual_cog_copy_variants_and_options_persistence_replay` passed at `parity-models.log:1710`.

Caveats:

- Qt PNG captures closed-cog Manage Notes panel, no open cog menu, Notes Options checkbox pane or viewer-hover screenshot. Behavioral recording covers those actions; no matching Qt PNG for native hover.
- Shared manage_notes.png shows editor/cog button with different tab/content state. No authored native open-cog-menu or Notes Options checkbox PNG found; do not claim rendered menu/checkbox parity.
- Native inline naming/questions, text labels and layout differ Qt icon toolbar/dialog. Rust cursor offsets are UTF-8 bytes; Qt offsets UTF-16; the regression translates actual Unicode insertion.
- Real cog popup existence/mouse routing and bound check model are asserted; checkmark visuals/menu separator are source-declared, not asserted by unsupported OS-menu introspection.
- Broader Manage Notes, viewer interaction and Notes Options parents remain Partial. Left-click note editing/right-click hide, keyboard Apply and empty-tab double-click are outside scope.
- Retired/hidden viewer copy boundary is covered by retained callback after actual close; independently hidden-active/rebound viewer branches are not separately asserted. Renaming/copying cursor retention has source support but no dedicated cursor-after-rename/copy assertion in notes_preferences test.
- Fresh native hover does not visibly paint fox emoji; the recorded Unicode clipboard and actual keyboard-insertion assertions pass. No font/glyph parity is claimed.

## audit-options-notes-start-editing-notes-with-the-text-cursor-at-the-end-of-the-document

**Approve scoped Linux leaf.** Recorded defining actions/settings reach actual native consumers; staged Apply/Cancel/reopen and applicable owner boundaries pass. Fresh available surfaces are usable within declared framework/fixture limitations; missing captures do not establish visual parity for uncaptured surfaces.

`manage_notes::notes_are_added_edited_and_deleted_as_the_reference_does` passed at `linux-full.log:2200`. `notes_preferences::options_cog_cursor_copy_and_live_hover_replay_with_owned_cancel_and_reopen` passed at `linux-full.log:2271`. `import::decode::tests::four_note_preferences_import_and_survive_reopen` passed at `linux-full.log:3911`. `notes_preferences::actual_cog_copy_variants_and_options_persistence_replay` passed at `parity-models.log:1710`.

Caveats:

- Qt PNG captures closed-cog Manage Notes panel, no open cog menu, Notes Options checkbox pane or viewer-hover screenshot. Behavioral recording covers those actions; no matching Qt PNG for native hover.
- Shared manage_notes.png shows editor/cog button with different tab/content state. No authored native open-cog-menu or Notes Options checkbox PNG found; do not claim rendered menu/checkbox parity.
- Native inline naming/questions, text labels and layout differ Qt icon toolbar/dialog. Rust cursor offsets are UTF-8 bytes; Qt offsets UTF-16; the regression translates actual Unicode insertion.
- Real cog popup existence/mouse routing and bound check model are asserted; checkmark visuals/menu separator are source-declared, not asserted by unsupported OS-menu introspection.
- Broader Manage Notes, viewer interaction and Notes Options parents remain Partial. Left-click note editing/right-click hide, keyboard Apply and empty-tab double-click are outside scope.
- Retired/hidden viewer copy boundary is covered by retained callback after actual close; independently hidden-active/rebound viewer branches are not separately asserted. Renaming/copying cursor retention has source support but no dedicated cursor-after-rename/copy assertion in notes_preferences test.
- Fresh native hover does not visibly paint fox emoji; the recorded Unicode clipboard and actual keyboard-insertion assertions pass. No font/glyph parity is claimed.

## audit-options-notes-when-middle-clicking-a-note-hover-only-copy-the-text

**Approve scoped Linux leaf.** Recorded defining actions/settings reach actual native consumers; staged Apply/Cancel/reopen and applicable owner boundaries pass. Fresh available surfaces are usable within declared framework/fixture limitations; missing captures do not establish visual parity for uncaptured surfaces.

`manage_notes::notes_are_added_edited_and_deleted_as_the_reference_does` passed at `linux-full.log:2200`. `notes_preferences::options_cog_cursor_copy_and_live_hover_replay_with_owned_cancel_and_reopen` passed at `linux-full.log:2271`. `import::decode::tests::four_note_preferences_import_and_survive_reopen` passed at `linux-full.log:3911`. `notes_preferences::actual_cog_copy_variants_and_options_persistence_replay` passed at `parity-models.log:1710`.

Caveats:

- Qt PNG captures closed-cog Manage Notes panel, no open cog menu, Notes Options checkbox pane or viewer-hover screenshot. Behavioral recording covers those actions; no matching Qt PNG for native hover.
- Shared manage_notes.png shows editor/cog button with different tab/content state. No authored native open-cog-menu or Notes Options checkbox PNG found; do not claim rendered menu/checkbox parity.
- Native inline naming/questions, text labels and layout differ Qt icon toolbar/dialog. Rust cursor offsets are UTF-8 bytes; Qt offsets UTF-16; the regression translates actual Unicode insertion.
- Real cog popup existence/mouse routing and bound check model are asserted; checkmark visuals/menu separator are source-declared, not asserted by unsupported OS-menu introspection.
- Broader Manage Notes, viewer interaction and Notes Options parents remain Partial. Left-click note editing/right-click hide, keyboard Apply and empty-tab double-click are outside scope.
- Retired/hidden viewer copy boundary is covered by retained callback after actual close; independently hidden-active/rebound viewer branches are not separately asserted. Renaming/copying cursor retention has source support but no dedicated cursor-after-rename/copy assertion in notes_preferences test.
- Fresh native hover does not visibly paint fox emoji; the recorded Unicode clipboard and actual keyboard-insertion assertions pass. No font/glyph parity is claimed.

## Images actually viewed

- `/workspace/validation-reviews/ci-ff858ded1/native-renders/notebook_tree.png`; SHA-256 `ed7e6cc1aae5f9a0190a050d5574cdc7a1d6cd6f4dce7db3785e94f285ec972a`. 900x600 Main: Left tree has alpha/inner expanded; six visible names alpha, one, inner, beta, gamma, omega; inner blue-selected with branch disclosure. Matches final Qt tree state; Collapse all/Expand all readable; navigation tabs hidden. Full-window/sidebar metrics differ Qt widget-only capture.
- `/workspace/validation-reviews/ci-ff858ded1/native-renders/notebook_tabs_hidden_1.png`; SHA-256 `2513de07aed64dfe02307ba8380e2bc8110ac4ecab4230c0cf46bca6bd7281ed`. 900x600 Main: Left hierarchy visible with gamma blue-selected, branch disclosure and Collapse all/Expand all. Actual navigation tab rows hidden. Five long labels elide to sidebar width; this is supporting tab-presentation fixture, not the six-name Qt tree fixture.
- `/workspace/validation-reviews/ci-ff858ded1/native-renders/notebook_tabs_hidden_2.png`; SHA-256 `750f02f8e2b3c2f66065f26b3584982941acd0b4a87f597cfcf09425958403ce`. 900x600 Main: Right hierarchy visible with gamma blue-selected and branch disclosure; navigation tabs hidden. Sidebar/toolbar contained; long labels intentionally elide. Supporting fixture differs primary Qt tree.
- `/workspace/validation-reviews/ci-ff858ded1/native-renders/manage_notes.png`; SHA-256 `a69e022feb7296aa6f4f00ee9673be96039d148df13b68a8efe81e37d6b2fef0`. 640x420 shared editor: notes/source tabs; notes blue-selected; first line/second text and caret; readable add, edit current name, delete current note, copy, paste, copy URLs, cog, apply/cancel. No overlapping control or clipped action. Closed-cog shared editor differs Qt selected-empty/four-tab state; comparison is controls only.
- `/workspace/validation-reviews/ci-ff858ded1/native-renders/note-hover-copy.png`; SHA-256 `b30b985d9f887b6703b122427cc514fbecdc66eaa33793cd9e4c7222b0ede046`. 800x600 viewer: visible dark note-hover at upper right with empty/note10/note2 labels and body; wrapped note10 URL, note2 two. This is after actual second middle press consumed saved title+body preference. No open menu/Options shown. Fox emoji in Unicode note text is not visibly painted in this font; exact Unicode clipboard output and insertion are tested, not glyph parity.

- `oracle/fixtures/notebook_tree_qt.png`; SHA-256 `74d05c75b661eebb40841ea96f74609c0f94655f20236d0c35940aab77641982`. Actually inspected: tree widget only, inner blue-selected, alpha/inner expanded, six visible named rows. No toolbar, Options pane or full client.
- `oracle/fixtures/notes_preferences.png`; SHA-256 `37b07679b82771999ea0aa47321796f4c5f9567c3f84331643e48b909a31a50c`. Actually inspected: Manage Notes panel with empty selected; tabs empty/note10/note2/future, blank text cursor at start, bottom add/rename/delete/copy/paste/link/cog controls. Cog closed; no Options checkboxes or viewer hover.

Detailed file/line evidence and full test-log proof are in the companion JSON. The absence of native open-cog/Notes Options captures and a Qt viewer-hover capture is preserved; no rendered parity is asserted for those surfaces.
