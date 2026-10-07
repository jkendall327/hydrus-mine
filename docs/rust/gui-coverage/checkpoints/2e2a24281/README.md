# Validated archive timestamp repair

Exact source `2e2a2428112bdeb17a9fb9642e7a43c2934d2325`, full Linux run
[37682619326](https://github.com/jkendall327/hydrus-mine/actions/runs/37682619326).
This checkpoint signs off exactly one existing reference leaf:
`audit-media-menu-database-fix-missing-file-archived-times`.
The total becomes **374 signed off, 1 existing candidate pending**. The previous
373 sign-offs remain intact; no native, parent or alias credit is added.
The 1,274-leaf goal has 900 leaves outside the explicit completion ledger.

Strict Clippy and all 2,197 workspace tests passed, including 713 GUI and 67 media
tests, with zero failures and three existing intentional ignores. Both new GUI
tests and the complete original archive GUI regression passed. Its original file
remains an exact prefix; store/model tests, recorder and fixture are unchanged.
A fresh real Qt/database recording reproduced the fixture exactly, including
actual questions, repair operations and reopened archive timestamps.

Long explanations now wrap to the native scroll viewport. At 440×480 the entire
population question fits; at 440×360 it genuinely overflows and ordinary wheel
input exposes the lower explanation while the action buttons stay visible.
Default 680×480 and wide 840×640 captures are also readable. Fourteen new frames
cover the warning, all population choices and outcomes, narrow/short/scrolled/wide
layouts, and a reopened scan with no missing times. Root and independent review
inspected eleven frames directly; the remaining four are hash-identical to their
inspected counterparts. The original regression render was inspected too.

Cancellation retains the repair worker's authoritative result. A request before
writing rolls back without refreshing media; a commit completed before the UI
handles cancellation still reports Done and refreshes media. Physical dialog
buttons exercise warning decline, all three populations and completion. Menu
navigation, cancellation ordering and ownership are callback-driven regressions.

Native warning, choices and progress share one owned window instead of Qt dialogs
and a job popup. Native cancellation rolls back the entire transaction, whereas
Qt can retain earlier completed batches. Captured-candidate revalidation preserves
intervening edits. Closing after commit cannot undo it. The reference recorder
does not capture matching Qt dialog pixels. Windows/macOS remain deferred.

Failed and cancelled runs remain diagnostic history. The initial Slint wrapper
failed compilation because a Button cannot contain the measuring Timer; a wrapper
Rectangle preserves button geometry. The subsequent run passed 712 GUI tests but
incorrectly expected overflow where all text fitted. The repair retains that
440×480 capture and every assertion, adding the genuinely constrained viewport.
Only the successful exact-source run above grants credit.

The package-mirror edit did not reliably switch the runner's effective mirror:
the cancelled run resolved `apt-mirrors.txt` and still downloaded from Azure.
That infrastructure follow-up is separate from archive parity. The successful
retry took 298 seconds to install packages, 615 seconds for strict Clippy and
1,247 seconds for the full test step. It used dependency caches, found no compatible
workspace snapshot, and saved a new snapshot. These are observations, not a
performance guarantee.

The packet retains exact-source CI, independent and root rendered reviews,
reference replay, historical diagnostics, publication fingerprints, browser checks
and the preservation audit. Cumulative progress since 240 is 134 across twenty-four
checkpoints; it is not a rolling 24-hour sign-off count.
