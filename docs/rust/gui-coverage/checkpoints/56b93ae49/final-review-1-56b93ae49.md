# Final independent review: assigned nine controls

Exact source `56b93ae49337356aa4f81c79b12171397278247b`, Linux run `37425380545`, native artifact `11394399260`. All nine assigned selected IDs are eligible for bounded Linux publication. No blocker found; final publication remains root’s decision. Required Linux gate was independently checked with the committed `gui_publish.validate_ci`; required job steps are green, GUI696passed0failed. No local build or canonical edit.

Actual `view_image` inspection:

- Native `/workspace/validation-reviews/ci-56b93ae49/native-renders/files-view-removal.png` (`46a4ef78336105b9947b591459f9497f0b58e9b8bc4f69cd74fd7cd02b7807b0`) against committed Qt `/workspace/hydrus-mine/oracle/fixtures/files_view_removal.png` (`d0a7cff6dcf9a6996310a78d0b14d62fb19485a86f40853a88dc948ad5d59e34`).
- Native `/workspace/validation-reviews/ci-56b93ae49/native-renders/options_import_work_slots_loaded.png` (`297dfac86240bb2f58ef533e134ca3d2b8c29dc51331e48333b43851830a3897`) against committed Qt `/workspace/hydrus-mine/oracle/fixtures/import_work_slots.png` (`d7bcf576498bcffc4c49d3b7a89d4a1ef6e12f3283ed39fca45c5887b99082b9`).

All four Files and Trash removal controls are visible, readable and checked, matching the actual Qt panel snapshot after its16-combination replay. Order and exact mapped labels agree. The five importing work-slot fields are all visible with values1,1,500,500,1 in gallery-files/gallery-search/watcher-files/watcher-check/misc order, exactly matching Qt loaded-boundary capture. UI fonts, row height, palette, outer Options navigation and section layout differ as expected. These are scoped control-state comparisons, not pixel identity.

Eligible original IDs:

- `audit-options-files-and-trash-even-skipped-files`
- `audit-options-files-and-trash-remove-files-from-view-when-they-are-archive-delete-filtered`
- `audit-options-files-and-trash-remove-files-from-view-when-they-are-moved-to-another-local-file-domain`
- `audit-options-files-and-trash-remove-files-from-view-when-they-are-sent-to-the-trash`
- `audit-options-importing-work-slots-number-of-gallery-downloader-file-queues-that-can-import-at-the-same-time`
- `audit-options-importing-work-slots-number-of-gallery-downloader-searches-that-can-run-at-the-same-time`
- `audit-options-importing-work-slots-number-of-other-paged-importer-jobs-that-can-run-at-the-same-time`
- `audit-options-importing-work-slots-number-of-watcher-page-checkers-that-can-run-at-the-same-time`
- `audit-options-importing-work-slots-number-of-watcher-page-file-queues-that-can-run-at-the-same-time`

Source/behavior verification: Files and Trash actual Options Apply/Cancel/reopen, dependent skipped-enabled state, retained old callbacks, captured filter Accept/Forget/Resume/hidden/closed/page-switch no-ops, actual trash membership/advanced deletion/locked no-ops, domain move strict/merge/Copy/cancel/service failure, retained rebound children and forgotten viewer source are asserted in exact-source native tests and passed. Model replays preserve actual Qt commit/pruning matrices; legacy/typed import passes. Import controls replay actual draft/save/reopen bounds, raw loaded-value Cancel and Apply normalization; all five pool categories replay actual controller acquire/lower/raise/release boundaries. Real runner tests pass live file limits, pending/no-permit, cancel/owner close, gallery-search growth/error release, independent watcher-check capacity, legacy import and monotonically allocated owner-qualified queues. Exact test/log/source anchors are recorded in the JSON.

Limits and visual caveats:

- Native uses full Slint Options shell, larger fonts/row heights, blue check marks and different panel grouping; Qt captures only their respective panels. Compare the nine mapped labels/values/control states, not full-frame pixel identity.
- Files capture is all four removal flags true after the16-case settings replay, not default-state proof. Default-false and disabled-skipped behavior come from actual fixture/source/passing assertions. The unrelated multiple-deletion-delay checkbox differs across fixtures; that scope is not selected or approved here.
- Qt importer panel contains red advanced warning and pending/working explanatory copy absent from this native frame. All five selected integer controls, labels/order/loaded-boundary values are visible and match; parent Importing layout/help parity remains outside these five controls.
- No image proves dynamic scheduling/pruning by itself. Those behaviors are supported by exact-source native/model/store/downloader pass records and the reviewed assertions.
- Files and Trash parent stays Partial; external/API/out-of-process broadcasting to every open page and broader deletion/service-choice/undelete/filtering families remain outside these four controls.
- Importer slots are runner-owned categories, held during bandwidth/network waits and synchronous imports. Local owner close can finish current blocking import before release; pending workers reread within one second and network close cleanup is polled within250ms.
- Scheduling cadence/FIFO fairness, subscriptions, global pauses, unrelated importing controls and broader Importing/page families remain Partial.
- Only Linux validated under immutable owner policy; Windows/macOS deferred.

Machine-readable ID eligibility, exact image hashes/source anchors and22 scoped passed test records: `/workspace/validation-reviews/final-review-1-56b93ae49.json`.
