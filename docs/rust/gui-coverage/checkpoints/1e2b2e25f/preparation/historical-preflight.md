# Nested File queue: next three-leaf preflight

Read-only preparation at `791b72d194d93e637aeeb0a243c81a9981cf41aa`, while Linux run `37599290504` is active. No implementation, builds, reference reruns, approvals or ledger changes. Current published checkpoint remains 359 accepted / 16 pending; the active Defaults + URL pair is excluded from this recommendation.

Recommend one cohesive, test-only three-leaf batch after the current checkpoint banks:

- `audit-options-nested-open-file-call-list-choose`
- `audit-options-nested-open-file-call-list-add-edit`
- `audit-options-nested-open-file-call-list-order`

All three entries are in `docs/rust/gui-coverage/parity/open-externally.json:732,870,1008`. They share `OpenFileCallsWindow`, the detached `Queue`, and the routing owner's registered-call chooser/question slots. No production defect is demonstrated by this preflight. Do not include the separate outer MIME Add/Edit/Delete leaves.

## Existing evidence and precise gaps

`crates/hydrus-gui/tests/gui/open_externally.rs` currently contains **156 assertion macros / five tests**. Preserve every existing assertion and statement, including the active URL additions. The main replay at line 528 already covers new/existing File child Cancel/Apply, Add append, Edit exclusion including the edited call, an explicit one-choice chooser, Down, exhausted Information, captured Delete No/Yes, blocked parent Apply/navigation and staged Store preservation (nested section lines 665–776). Full parent snapshots and final saved/reopened routing are already checked. Helpers `files`/`add_nested` are at 413/422; reuse the settled real-render helper at 130–169.

The actual reference recorder, `oracle/record_open_externally.py:124–140`, additionally records **eight `nested_checks`**: initial, Edit Cancel, Edit OS, Edit restore, Down, Delete Cancel, Delete Yes, exhausted Edit after re-adding the registered calls. No Rust source currently reads `nested_checks`. Native nested choices mostly check counts rather than complete recorded labels, and there are no defining nested child exports. Strengthen the existing replay or append one focused test to compare every recorded complete ordered name/key vector, selection, and chooser title/description/full labels. Compare keys through fixture-to-manager CallRefs and saved routing; internal native row IDs are not exposed by the GUI. The shared model tests at `crates/hydrus-gui-model/tests/model/open_externally.rs:66` cover URL queue semantics, not these eight File snapshots. Source `Queue::put` preserves row ID on Edit; `move_selected` retains identity/selection (`src/open_externally.rs:40–112`). GUI full cells/selection and saved stable callable keys provide observable evidence without adding hooks.

For Order, the real Main/live-viewer process consumer at GUI line 1073 currently **seeds saved routing directly**. Add a narrow connection: edit a File queue through the ordinary child, move the selected call to first, Apply child + parent, reopen and compare complete CallRefs, then use the existing owned-process/launcher consumer to prove that first saved call is selected. Preserve the current MIME precedence, explicit-empty fallback and exact typed-argument assertions. Avoid a new launcher feature or additional OS launch breadth. An Up/Down round trip can establish both arrows with unchanged final fixture order; the recorded finite sequence itself exercises Down.

## Minimum defining exports and ownership additions

Use six fresh live captures, taken before later child creation changes the collector's last adapter:

1. `file-call-chooser-registered.png`: complete three eligible File choices in recorded sorted order, not the MIME chooser.
2. `file-call-queue-populated.png`: actual nested queue with `File one 日本` + `File two`, selection and enabled controls.
3. `file-call-edit-single-choice.png`: explicit one-choice Edit child, complete label/title/description.
4. `file-call-remove-question.png`: exact owned `Remove 1 selected?` before answering.
5. `file-call-queue-reordered.png`: selected call after Down; complete observable row order plus identities checked before exporting.
6. `file-call-exhausted-information.png`: exact File exhausted text, before dismissal.

The existing parent export/reopened checks remain supplemental. Require bounded timer/event pumping and inactive animations with real rendered frames; do not force flags or export a closed parent under a child filename. Capture dimensions must be recorded. `OpenFileCallsWindow` prefers **580×340**, registered choice **520×440** (`ui/open_externally.slint:37–74`). Start there; inspect wrapping, Unicode, rows, arrows and footer in fresh artifacts. Resize only if genuinely needed and state the supported dimensions rather than claiming universal geometry.

Add a bounded retirement helper for old nested File window plus retained choice/question: cancel old Options, open a current successor with a genuinely pending nested choice/question, invoke old chosen/answer/Cancel/Apply/action/native-close routes, then assert full saved `(Routing, Manager)`, complete parent and child rows/selection, current slot identities/visibility, and drafts unchanged. Positively accept the current successor choice/question and prove the exact expected staged change, then Cancel preserves Store. Also hide/re-show a live chooser to prove hidden admission is a no-op while current visible acceptance works. Reuse the active URL helper's full draft/Store comparison pattern, without new automatic Main destruction/final Bound scope.

## Actually inspected historical images and geometry limits

- Native `/workspace/validation-reviews/ci-625a2bdaf/native-renders/open-externally-routing.png`, **960×800**, SHA256 `b1275d50ca0ee5fc529d38d59d91044fa7e0fb64a29964fec5e534f4a69248a4`. Manifest binds historical source `625a2bdafc5b5820f4d9201f46640d7770fbecd7`, run `37590137304`, artifact `11468462915`, zip SHA256 `2e06c51d1ce0d51550e31d0dccce4de69c3902d814d01f3146eeaf5a34c14228`. Reopened with `view_image` for this preflight: Unicode PNG row is readable; both MIME rows are selected and File Edit/Delete are visibly disabled. Approximate File Edit bounds x457–696/y589–620 apply only to this historical layout. A finite physical parent Edit route would first need PNG-only selection, exact enabled state, settling, and fail-closed resulting File child title/rows. It cannot be justified by clicking the currently disabled historical button.
- Qt `/workspace/hydrus-mine/oracle/fixtures/open_externally.png`, **950×800**, SHA256 `93e6891da43fa4d641c8bc80e7d711a3191a1aaad58b64fb5fe026ec071b8aec`, actually reopened. This is the later washed/renamed/missing outer panel, not a nested chooser/queue state or a matched native pixel pair. The recorder uses scripted dialog substitutions for these finite choices/questions. No nested Qt screenshot or established child button coordinates were found.

No Slint observation hook is needed for the existing callback-route scope. Do not guess nested child pointer coordinates before a genuine child frame exists. If physical nested buttons are additionally desired, scope them to a supported viewport with independently verified hit geometry and outcome, or report the missing evidence separately; do not enlarge this three-leaf repair into generic geometry infrastructure.

Retain structural parents/aliases as Partial with zero extra credit, column sorting/persistence and inherited full keyboard controls excluded, no broad multi-selection/OS process-error/platform claims. All next implementation and fresh Linux/render approval remain gated on banking the active checkpoint.

## Exact-source hashes

- GUI `open_externally.rs`: `b8c2fba97967a94fb95480a86477744f0ed5b5d2504879dcfc063dc3d64ce35c`
- `src/options_open_externally.rs`: `2625eee8d92548378ae1c5f5063b015d8f031b22f8fdf75dbb9d5b61340f33d1`
- `ui/open_externally.slint`: `d87ecd0c356b2e7cc7d90b442e1f15bada574e3a0b929f6520ba58c39be2e674`
- Recorder: `35ef866834dd401e913c5a022b72c444bb0fedf309a8d715203c8d301f749bc2`
- Committed reference JSON: `37b7f14e70fd811dd6e021968ab8ebb8123d235781aa91ea19102966b6f5c95f`
- Manifest: `20932740466dc6473472bdda5651a8c8d7b22f10feb23965e6fee6f75360549c`
