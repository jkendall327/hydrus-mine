# Next outer MIME Add/Edit/Delete preflight

Read-only at exact `1e2b2e25fd89b24253f4387f71115e675fdb657b`, while File queue Linux run 37605060836 is active. No edits, builds, recorder reruns, approvals or ledger changes. Published checkpoint stays 362 accepted / 13 pending; bank the current File queue three first.

Recommend a cohesive next three-leaf batch, with one demonstrated prompt repair plus bounded test additions:

- `audit-options-open-externally-single-file-calls-add`
- `audit-options-open-externally-single-file-calls-edit`
- `audit-options-open-externally-single-file-calls-delete`

Manifest: `docs/rust/gui-coverage/parity/open-externally.json:318,456,594`. Same routing owner/Store/fixture and existing test file; preserve all current 249 assertion macros/six tests, including URL and File queue consumer proofs.

## Actual reference and existing native gaps

`oracle/record_open_externally.py:76–101` drives real modal child construction and records events8–13: Add child Cancel, Add child Apply, Edit child Cancel, Edit child Apply, protected GeneralFile deletion, and accepted PNG deletion. `nested` records the actual new/existing titles and complete before-answer CallRefs; chooser4/6 record exact remaining MIME tuples in general-group then searchable-MIME order (`sort_tuples:false`). PNG is selected for Edit and positive Delete. The later mixed-GeneralFile selection protection is separately recorded as `multi_general_protected:true` (lines138–140). Qt source `OpenExternallyPanel.py:267–345` preserves these staged Add/Edit outcomes; Add selects/sorts/scrolls the new row, Edit replaces the selected MIME queue.

Native main replay `tests/gui/open_externally.rs:665–776` already compares complete events8–11, chooser MIME labels and nested Cancel/Apply, and blocks mixed GeneralFile deletion. It does **not** replay event12's standalone protected GeneralFile selection or event13's positive outer Delete. It does not inspect/capture the outer MIME chooser/new blank queue/existing Edit child/outer deletion question, explicitly compare the post-Add selected sorted MIME identity, or directly exercise pending outer MIME-choice/Delete-question retirement against a usable successor. Existing File queue captures and retirement are useful shared supplements, not substitutes for outer-route evidence.

## Concrete product blocker: outer deletion question

Qt constructs this table with `use_simple_delete=True` and `_GeneralFileIsNotSelected` at `hydrus/client/gui/panels/options/OpenExternallyPanel.py:132–138`. `ClientGUIListCtrl.py:1156–1171` admits deletion through that guard, then `ShowDeleteSelectedDialog:1371–1378` **always asks `Remove all selected?`**. The actual PNG-only deletion at recorder lines98–101 records exactly **`fixture["notices"][2]`** and event13, while GeneralFile remains present.

Native `src/options_open_externally.rs:597–610` uses selected-count versus total-row count; PNG-only selection among GeneralFile+PNG therefore asks `Remove 1 selected?`. This contradicts the recorded outer Delete claim. Minimal repair: change the **outer `file_action` Delete prompt only** to the exact reference string, retaining captured MIME identities, guard, decline and staged acceptance. Leave nested `queue_action` count-based `Remove N selected?` unchanged; it matches recorded notices4/5 and the current active batch.

## Smallest meaningful next test and capture plan

Use the existing main replay or a focused appended test, retaining every prior assertion:

1. Compare complete remaining MIME tuple order/title, excluding GeneralFile and used PNG when applicable. MIME chooser Cancel preserves full draft/Store; selecting PNG opens actual blank `edit calls`. Assert child Cancel leaves event8 unchanged; child Apply produces exact event9 including the newly selected sorted MIME row.
2. Select that existing PNG; assert actual `edit launch path` queue values, Edit child Cancel event10 and child Apply event11 with unchanged MIME identity/table order. Compare full saved typed Routing CallRefs and Manager after parent Apply and reopen, not visible labels as opaque-key evidence.
3. Replay standalone GeneralFile protection event12 and recorded mixed protection. PNG-only Delete must first assert exact notices2; No preserves complete rows/selection/Store, Yes yields exact event13, staged-only until Options Apply. Cancel discards; a separate Apply/reopen verifies the precise removed MIME key with all other Routing/Manager fields preserved. Outer No is source-grounded in `ShowDeleteSelectedDialog`'s accepted-only branch; the committed recorder's outer Delete outcome is Yes.
4. Retain a genuinely pending old outer MIME chooser and old captured-MIME Delete question through Options Cancel. A live successor's route kind, complete draft/selection/Store/Manager, slot identity and visibility survive old chosen/Yes/No/Cancel/force-close/root actions. Positively choose/answer the current successor and prove the exact staged change, then Cancel preserves Store. Reuse existing helpers; no automatic Main/final Bound destruction credit or new production hooks.

Six proposed live defining exports: `mime-add-chooser.png` (520×440; full model order asserted even though list scrolls), `mime-add-blank-calls.png` (580×340), `mime-edit-existing-calls.png` (580×340 with Unicode rows), `mime-mapping-reopened.png` (960×800 selected PNG/table), `mime-delete-question.png` (520×200 exact outer prompt before answer), and `mime-delete-staged.png` (960×800 only unchanged GeneralFile remains). Retain each constructor's actual adapter and settle real timers/animations/rendering using the current helper. No closed-parent screenshot under a child filename. Readability, Unicode, flags and footer require fresh hosted image review.

Historical native `ci-625a2bdaf/native-renders/open-externally-routing.png` is 960×800, source `625a2bdafc5b5820f4d9201f46640d7770fbecd7`, run `37590137304`, artifact `11468462915`, SHA256 `b1275d50ca0ee5fc529d38d59d91044fa7e0fb64a29964fec5e534f4a69248a4`; previously actually inspected. It has both MIME rows selected, so Edit/Delete are disabled, and cannot prove a positive pointer route. Callback scope suffices for this bounded plan; no guessed child coordinates or Slint observer. Qt `oracle/fixtures/open_externally.png`, SHA256 `93e6891da43fa4d641c8bc80e7d711a3191a1aaad58b64fb5fe026ec071b8aec`, shows a different later washed outer state, not matching new child/question pixels. No fresh image inspection or pixel parity is claimed here.

Preserve broader parent/alias Partial boundaries, absent column persistence/full keyboard/deeper launch/import/platform behavior. Do not expand into all-MIME exhaustion or general geometry infrastructure. The prompt mismatch is a narrow next-batch repair; it does not invalidate the current correctly worded nested File queue test.

Exact hashes: GUI test `b5c735bf879e017c0aa621c27a9f4461b6f65ac646f43cd790e636bbf0b07cdb`; routing production `2625eee8d92548378ae1c5f5063b015d8f031b22f8fdf75dbb9d5b61340f33d1`; Slint `d87ecd0c356b2e7cc7d90b442e1f15bada574e3a0b929f6520ba58c39be2e674`; recorder `35ef866834dd401e913c5a022b72c444bb0fedf309a8d715203c8d301f749bc2`; JSON `37b7f14e70fd811dd6e021968ab8ebb8123d235781aa91ea19102966b6f5c95f`; manifest `20932740466dc6473472bdda5651a8c8d7b22f10feb23965e6fee6f75360549c`.
