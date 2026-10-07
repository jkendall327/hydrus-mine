# Suggested-tags width/layout2: source review

No concrete compile/API, runtime-order or assertion-preservation blocker identified in the current two-file patch. No source/claim/render approval is issued. Root alone edits/builds; exact committed-source full Linux validation and fresh image review remain required.

Baseline HEAD `a95eeefd638eb806efc91bd81cdb30248b888b0f`; selected dirty test diff SHA256 `1000431dae5196d15502b980b153a663ba11efd31660262052d44cedd8195e9e`. Individual original/current hashes and preservation proof are in JSON.

## Existing production APIs and Rust types

No concrete compile/API/lint blocker found by source inspection. Existing Rc/Cell measurement callback arguments match Options choice-state-measured(int,MenuChoiceFrame,int,string,bool); public setters/getters and headless render_snapshot/save_png exist. Existing Model imports cover row_data/iter. ServiceId and TagId use the same SQL parameter forms as production recent writes; intern_tag takes &Tag, TimestampMs::now().0 gives used_ms. New notebook Store::write closure captures no borrowed locals, avoiding the prior static-closure issue.

Source: `crates/hydrus-gui/tests/gui/options_window.rs:277,281,310`, `crates/hydrus-gui/ui/options.slint:237,595,609`, `crates/hydrus-store/src/master.rs:196`, `crates/hydrus-store/src/schema.rs:521`, `crates/hydrus-gui-model/src/manage_tags.rs:712`.

## Actual saved parent ComboBox state

The capture follows real parent Apply, reopened Store saved-width240/columns/defaultrecent assertions, then real reopening of Options. Fresh row.kind5/index1/items side-by-side and 1ms real ComboBox observer are checked with positive frame/enabled/current selected caption. Bounded2s draw+10ms sleep loop drives timers without replacing captions/model; snapshot captures the same indexed freshly opened parent before another child is created. Observation is disabled before continuing all original child Cancel/stale Apply/service tests.

Source: `crates/hydrus-gui/tests/gui/options_window.rs:245,264,274,277,297,300,310,320`, `crates/hydrus-gui/ui/options.slint:595-610`, `crates/hydrus-gui-model/src/options.rs:4704-4721`.

## Real recent fixture and populated columns

The seed asserts nonempty selected files and that parity:recent is absent on every selected file, then interns/persists actual recent_tags for my tags in the same Store writer. This is fixture input matching Qt push_recent_tags; the production reader/filter/publisher remain unchanged. Existing parity:present on all selected files stays filtered and most-used three rows/first parity:new2 checks remain. Both available row models are asserted populated before actual two-column render, with original exact width240 and selected-recent/columns checks retained. No related/lookup worker activation or manual row replacement.

Source: `crates/hydrus-gui/tests/gui/manage_tags.rs:58,60,80,104,116,123,129,133`, `crates/hydrus-gui-model/src/tag_suggestions.rs:52,90`, `crates/hydrus-gui-model/src/manage_tags.rs:604-625`, `crates/hydrus-gui/src/manage_tags_window.rs:263-290`, `oracle/record_tag_suggestions.py:28-31,65-88`.

## Original observer/Apply/retirement assertions

The columns export occurs before original first click/activation. The repeated only-add, staged mapping, filtered removal, persisted broadcast,220ms wait+timer update, applied mappings, retired callback refusal and saved most-used length5 assertions are unmodified. No fixture changes current mappings or most-used list values. The recent seed remains useful after Apply because new2/present are now on all selected files and filtered while parity:recent is not.

Source: `crates/hydrus-gui/tests/gui/manage_tags.rs:143-199`, `crates/hydrus-gui/src/manage_tags_window.rs:337,474,684-713`, `crates/hydrus-gui-model/src/manage_tags.rs:712-715`.

## Fresh actual notebook owner and cleanup

Only after all original retirement assertions does the new code assert slot empty, save columns=false while retaining width240/defaultrecent/recent availability, and assert the retained predecessor still has opening columns=true. Production menu action constructs a new owner from persisted settings because slot is empty. New actual owner asserts columnsfalse, exact width bits240, selected recent1, both pages enabled, most-used nonempty and recent parity:recent present; its last-created adapter is rendered before Cancel. Cancel must empty slot before main hide. Existing service-switch fallback test with recent=None remains unchanged.

Source: `crates/hydrus-gui/tests/gui/manage_tags.rs:200-241,246`, `crates/hydrus-gui/src/manage_tags_window.rs:132-137,552-582`, `crates/hydrus-gui/src/lib.rs:1998-2029,2140-2175`.

## Bounded scope and honest capture proof

Exactly two existing GUI test files change; no production API/layout/source/canonical status expansion. The three images target reopened240/side-by-side Options, two populated columns and a fresh notebook recent page. Physical list width still has no dedicated measurement observer: saved240 property and intrinsic source constraints must not be described as measured rendered geometry. First-draw consumer captures and all checkbox/caption/list readability require actual fresh image inspection; no rendered approval is inferred from source.

Source: `crates/hydrus-gui/ui/tag_suggestions.slint:23,31-49`, `crates/hydrus-gui/ui/list_table.slint:25-29`, `crates/hydrus-gui/tests/gui/manage_tags.rs:133,230`.

## Original assertions

All 717 original assertion macros remain in source. The only removed original source line renames `_headless_windows` to `windows` to access the retained real adapters; no assertion line is removed or rewritten. Current assertion count 737. The existing recent-disabled notebook fallback test remains unchanged.

## Scope limits

- Only width and notebook/columns choice are in scope; no parent/alias/default-page/recent/related/lookup or most-used-editor completion credit.
- Existing owners retain opening preferences and availability; persisted most-used broadcasts use200ms observer. All existing cancellation/stale-owner/persistence tests are preserved.
- Native per-service editing differs from Qt inline editor; recent Clear/confirmation/read-time decay/settings editor remain outside this packet. Independently published unrelated leaves receive no additional credit here.
- Qt columns PNG is postactivation and includes Clear; proposed native columns capture is preactivation, notebook has extra persisted broadcast row from retained assertions. Label these states honestly rather than alter fixture results for pixel resemblance.
- Existing native CJK glyph rendering differs from Qt; fresh list/column/Options visual review must retain text/font/framework caveats and distinguish them from width routing.
- No builds/lints/runtime or fresh PNG inspection performed for this dirty patch. Exact committed-source full Linux CI and independent actual fresh render inspection remain required; Windows/macOS deferred.
