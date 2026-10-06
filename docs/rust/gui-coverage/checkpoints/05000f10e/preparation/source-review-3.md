# Presentation/appearance five: source review

No remaining concrete source/API, ordering or assertion-preservation blocker was found in the current dirty snapshot. **No source or claim approval is issued.** Exact committed-source full Linux CI and fresh actual image inspection remain required.

Baseline HEAD `ea159b6ae2d0a87d08e6dd0a8f4616911bbdd483`; selected four-test-file diff SHA256 `a437468f7869dda5ba7673afd968c386a82274661e2add2450f276e5a1682616`. Review is pinned to individual worktree hashes in JSON; root alone edited the repository.

Resolved during review: new settled-blurhash opacity assertion now uses exact `to_bits()` equality to `1.0_f32.to_bits()` rather than EPSILON tolerance. Root also added a bounded two-second actual-popup polling loop for the250ms refresh before production dismissal.

## Compile/API correctness

No concrete API/type/lint blocker identified by source inspection; no compiler or lint execution performed. Existing imports supply Rc, Duration, ComponentHandle and Model. Public Rows::set_paint_clock/paint_tick accept these types; Windows::get/count indexes are usize; save_png takes actual RGBA buffer/u32 dimensions. MainWindow fields and SessionDialog notice/title/OK getters exist in current Slint. PageKey implements Copy and Debug.

Source: `crates/hydrus-gui/src/grid.rs:469,504`, `crates/hydrus-gui/src/headless.rs:72,76,140,169`, `crates/hydrus-gui/ui/main.slint:276,278,302`, `crates/hydrus-gui/ui/session_dialog.slint:12,20,21`, `crates/hydrus-core/src/pages.rs:554,571`.

## Tab error and clear captures

Root current two-second polling loop waits for the real 250ms popup refresh by calling existing settle (real draw/timer update with bounded short sleeps). It asserts the actual invalid-choice error, then production dismiss-all refreshes synchronously and asserts zero popups/summary plus unchanged saved defaults. Every original selection, invalid-input, owner-refusal, orientation geometry, vertical glyph and side-change assertion remains.

Source: `crates/hydrus-gui/tests/gui/tab_presentation.rs:99,199,212,224`, `crates/hydrus-gui/src/popups.rs:440,552`, `crates/hydrus-gui-model/src/options.rs:787`.

## Sidebar actual hidden states

Existing render calls become saved buffers at existing asserted current-page sidebar-hidden and newly opened global-preview-hidden states. New zero-width/splitter-hidden/page-key assertions bind the images to the real selected page; production menu/Options paths, inactive-page restore, saved geometry and close/setup/lifetime checks remain. No properties are forced solely to create screenshots.

Source: `crates/hydrus-gui/tests/gui/sidebar_layout.rs:200,258`, `crates/hydrus-gui/ui/main.slint:931,1464`.

## ClearAll owner/ordering

Each adapter index is captured before actual database menu dispatch; the production opener creates one SessionDialog. Decline PNG is taken before answered(false), preserving all seeded SQL rows. Accept changes the same owner into notice_only Information/OK synchronously before its indexed completion render; SQL empty and reopened Store empty assertions remain. The title/notice assertions strengthen actual state proof. No live counter-refresh claim; completion still tells user to restart.

Source: `crates/hydrus-gui/tests/gui/viewing_maintenance.rs:66,77,141,151`, `crates/hydrus-gui/src/viewing_maintenance_window.rs:29,70,85`, `crates/hydrus-gui/ui/session_dialog.slint:20`.

## Blurhash real paint/default fade

New capture closure preserves missing real source/thumbnail precondition and target first id, asserts saved default fade still enabled, uses an owned clock and existing production draw/request/wait/tick route, advances two seconds beyond both finite paint durations, then asserts actual paint image bytes and exact finished-opacity bits before saving the real MainWindow. Enabled capture is before invalid metadata; disabled capture is after restored valid metadata and saved false Apply, preserving discrimination. Reopened Options derives false checked state from a fresh model; its newest adapter is created by that open and captured before Cancel/owner retirement. Original terminal cache assertions remain.

Source: `crates/hydrus-gui/tests/gui/thumbnail_appearance.rs:235,236,278,343`, `crates/hydrus-gui/src/grid.rs:469,475,504`, `crates/hydrus-gui/src/thumbnail_paint.rs:110,138,155`, `crates/hydrus-gui/src/lib.rs:779,792`.

## Background real Apply and extent

Existing post-Apply exact no-pink render assertion now names and exports its identical buffer; existing all-pixel/adjacent/resize assertions remain in the nonuniform31x17 test. One supplementary real-Store test saves absent background and admitted renderer choice, draws an empty actual MainWindow, then applies actual31x17 Qt-colour PNG through Options. It asserts exact image dimensions/all marker pixels/both adjacent unaffected pixels, exports the indexed real MainWindow, then retires/hides that owner before next independent mode. Default fade is untouched; explicit old/default-new fixture selection is not renderer completion.

Source: `crates/hydrus-gui/tests/gui/thumbnail_appearance.rs:529,846,861`, `crates/hydrus-gui/src/thumbnail_background.rs:8`, `crates/hydrus-gui/ui/main.slint:2019,2025`, `oracle/record_thumbnail_appearance.py:100,106,116`.

## Bounded scope and documentation

Only four existing GUI test files change behavior. GUI/DIFFERENCES additions explicitly describe pending captures, preserve signed-off334/pending41, and retain Qt whole-cell blending, renderer architecture, direct-Qt screenshot absence and background whole-window limits. No production or canonical manifest/ledger changes are part of this diff.

Source: `docs/rust/GUI.md:4577`, `docs/rust/DIFFERENCES.md:3699`.

## Original assertion preservation

The four files contained226 original assertion macros.225 remain after whitespace normalization; the remaining original exact no-pink assertion uses a named buffer from the same `headless::render` before exporting that buffer. No original assertion was dropped or weakened. The current snapshot contains252 assertion macros. New selected capture checks retain real Store/owner/worker/widget behavior.

## Limits

- Dirty-worktree review is bound by baseline HEAD, individual file hashes and selected diff hash; it is not final committed-source validation.
- No compilation/strict Clippy/runtime executed by this reviewer; actual timer/worker/Options row behavior and all new images must be confirmed in exact-source Linux CI and independently viewed fresh artifact.
- Blurhash fallback has actual Qt JSON/decoder byte evidence but no matching Qt visible-recovery or disabled-Options screenshot. Background Qt images have different viewport/content/framework states; only marker extent/anchor/clear policy can be compared.
- Fade and experimental renderer architecture stay Partial; no structural parents, aliases, interaction/scroll3, cache families or whole-window pixel parity credit. Windows/macOS remain deferred.
