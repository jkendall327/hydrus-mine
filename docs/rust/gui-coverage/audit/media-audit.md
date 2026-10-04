# Media, tags, local services and maintenance hierarchy audit

Inspected source snapshot: `5079206dc1d0c2d2614b428632dc9499cb9ad3f8`.

[media-patch.json](media-patch.json) contains full node updates and additions against that baseline: 124 native updates, 190 native additions, 53 reference updates and 283 reference additions, with no deletions. It leaves both canonical inventory files, snapshot metadata and totals untouched for integration. Existing IDs are preserved; new IDs use `audit.media.*` and `audit-media-*`.

This is source inspection, not a fresh behavioral test run. I traced concrete controls/callbacks, detached model state, persistence/worker consumers and the corresponding reference controls. Existing assertion bodies are cited with their actual scope and `executed_in_inventory: false`; a window declaration, filename or ancestor test does not establish a working leaf. All changed substantive nodes carry a plain-language assessment. `first_pass` applies to the specific described control, while incomplete families retain their missing/partial child work.

## What the hierarchy now exposes

- Media editors have separate service/tag count/toggle/autocomplete, note tabs/clipboard/options, rating kinds, timestamp/domain/service/on-disk changes, nested date editing, URL normalization, forced MIME/renaming and embedded section controls. Thumbnail/viewer contexts reveal selection/lifecycle/open/share/rearrange actions and missing maintenance, duplicate-group/king, viewing-stat cleanup, local-domain movement and URL-refetch branches.
- Archive/delete and duplicate canvases distinguish staged decisions, Back, commit/forget, custom merge and shared playback. Duplicate preparation, filtering-page search, rule maintenance, rule identity/search/action/preview, all comparator variants, nested AND/OR lists, review/history/progress and merge-note conflict options are separately represented. Shared rule/filter/viewer/merge editors use canonical links.
- Existing tag display/application/relationship/migration nodes now cite exact callbacks and state/write boundaries. Missing autocomplete/async/default-tab/reason-permission controls remain visible. Migration discloses its service-only/archive/hash/count/pause limitations, exact scope/filter freeze and actual worker commits.
- Local service add/delete guards, name/rating/API settings, staged registry Apply, service review counts, key administration, child permission validation/search filters and all fourteen grant rows are explicit. Grant checkboxes are assessed as GUI serialization; they do not imply complete associated endpoints. Remote repository/IPFS administration is recorded as an absent entrypoint without expanding that work, following the user's scope preference.
- Database mirrors every disabled menu action and expands physical storage weights/max-size/thumbnails/granularity/rebalance, file-maintenance queue/search/type/scheduling, deferred-delete review, vacuum, history chart, backup states, repair and cleanup workflows. Pending counts/confirmed forgetting are separate from missing repository upload.

## Concrete findings that change the map

Tags > migrate is disabled at `crates/hydrus-gui-model/src/main_menu.rs:939`. Working migration is reached through manage-tags/service review; that alternate path does not complete the menu action. The rule-editor equivalent of the reusable reference duplicate-search panel is partial; the filtering-page caller has no dedicated two-search/kind/pixel/distance/sort/group/quick-random controls (`crates/hydrus-gui/ui/duplicates_page.slint:170`, `crates/hydrus-gui/src/pages.rs:1646`, reference `hydrus/client/gui/pages/ClientGUISidebarDuplicates.py:36`).

The Database idle maintenance tick is persisted (`crates/hydrus-gui/src/menu_bar.rs:531`) but its daemon consumes only `during_active` (`crates/hydrus-cli/src/main.rs:616`). Similar-file discovery treats idle OR active OR work-hard as enabling search (`crates/hydrus-store/src/similar.rs:375`), without a GUI idle signal. Auto-resolution counts itself always active and reads only its active switch (`crates/hydrus-cli/src/main.rs:649`, `:669`). These are explicit partial scheduling paths, not green parity claims. No maintenance implementation was changed.

The tag single/selection filter nodes now cite the exact view-selection callback (`crates/hydrus-gui/src/tag_display_window.rs:226`), per-view assignment (`crates/hydrus-gui-model/src/tag_display.rs:119`) and store write (`crates/hydrus-store/src/tag_display_config.rs:121`). Existing tests accept/persist/reopen the single-file filter and check viewer refresh; the inspected selection-child test checks cancellation/stale ownership. The selection assessment explicitly discloses that an accepted-selection end-to-end assertion was not found. Corrected source-scope fields survive field-merge integration.

## Verification and handoff

Scratch merged inventories pinned to the inspected full SHA passed `scripts/gui_coverage.py` graph/status/source-anchor validation: native 530 nodes / 1,057 unique anchors and reference 533 nodes / 1,175 unique anchors. Shared/navigation targets and candidate native mappings resolved. This verification checks data/source integrity only; it does not test GUI behavior. Every owned originally unassessed menu/dialog/catalog node has a concrete assessment. The remaining unassessed nodes in the scratch inventory belong to other audit owners.

Only the patch and this audit are committed. No source changes, Cargo/build/test run, reference-tree mutation or push was performed. Integration should apply field updates/additions, repin common snapshot metadata, recompute hierarchy counts and render the map; it should not infer completeness percentages from the expanded node counts.
