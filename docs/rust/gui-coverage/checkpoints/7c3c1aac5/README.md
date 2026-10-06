# Linux checkpoint: 18 further validated completions

This checkpoint adds 18 independently reviewed original feature leaves to the
prior 301, for **319 signed-off completions**. Across four October 6 checkpoints,
79 completions have been banked; 56 candidates remain pending. Full Linux
validation passed at `7c3c1aac5a60a9e3fb1e7a8864cb439914d4fd60` in
[run 37447108755](https://github.com/jkendall327/hydrus-mine/actions/runs/37447108755).
Strict all-target Clippy, full workspace tests, reference/backend replays and the
parity ratchet passed. All 697 GUI tests and all 67 media unit tests passed, with
no failures or ignored tests in either suite. Windows/macOS remain deferred.

The [root review](review.json) binds exactly 18 new original IDs and six prior
wording corrections to the final patch and source census. [CI evidence](ci-evidence.json),
[workspace log](linux-full.log), [reference/backend log](parity-models.log),
[outcome](validation-outcome.json) and [selected patch](selected-patch.json) retain
the exact-source proof. The [preservation audit](preservation-and-copy-audit.json)
checks prior completion IDs/statuses/assessments, unselected reference nodes and
the native inventory. Source anchor remaps and ancestor counts are distinguished
from completion credit.

## Independent reviews

- [Favourites and downloader login exchange](final-review-2-7c3c1aac5.md): two leaves.
- [Image cache and viewer prefetch](final-review-3-7c3c1aac5.md): six leaves.
- [Downloader update timing and thumbnail cache](final-review-4-7c3c1aac5.md): six new leaves, plus fresh review of six prior wording corrections.
- [ICC and preview default zoom](final-review-5-7c3c1aac5.md): two leaves.
- [FFmpeg timeout and delayed popup](final-review-6-7c3c1aac5.md): two leaves.

Approvals cover only the selected finite behavior. Parent groups, aliases,
unrelated controls, pixel-identical Qt parity and deferred platforms gain no
credit. Verbatim review limitations remain in the inventory. In particular,
watcher timing controls are below the saved Options viewport, although their
native behavior passes; some consumer stills have no equivalent same-state Qt
capture. Preview flat-colour images prove geometry rather than decoded bitmap
parity, and native delayed-popup timing uses an injected clock plus real refresh.

The [native manifest](native-render-manifest.json) hashes all 274 exported PNGs
and their original ZIP. Actually reviewed selected/supporting images are retained
under `native/`; reference images remain under `oracle/fixtures`.

## Repairs and preserved failures

The preceding run at `115d5532a` passed lint/parity but failed two GUI checks.
[Failed-run evidence](failed-runs/115d5532a/validation-outcome.json) and its logs
retain that result. Numeric Options rows now reserve their controls' intrinsic
height instead of overriding a 30px-minimum spinbox with a 26px row. The existing
strict geometry test now passes at 900×640 and all three 1100px capture heights,
with both long and short helper text. Favourites replay now selects through the
actual dropdown popup; its native caption reads “my tags” alongside the accepted
tag list. Existing assertions and the two-second caption deadline remain intact.

Six previously published sidebar/tab limitations incorrectly described two
separate leaves as already banked. The explicit
[erratum](review4-banked-erratum-6b66240c1-at-7c3c1aac5.json) corrects those statements.
Neither separate leaf was in the 301-completion ledger. Counts and assessments
were correct and remain unchanged by this correction; archived reviews retain
their original hashes. The six affected prior leaves were freshly reviewed.

Clippy took 9m54s, test-profile compilation 10m08s and GUI execution 219.79s.
These are hosted observations, not controlled performance comparisons. The
bounded compilation investigation remains closed.

## Report and next work

The final report passed desktop and 390px Chromium checks for 319 cumulative IDs,
exactly 18 additional IDs, Linux/deferred-platform labels, inventories, filters,
searches, pinned source links and absence of errors/overflow. Root inspected both
captures. [Browser proof](browser/browser-check.json),
[desktop](browser/report-desktop.png) and [narrow](browser/report-narrow.png)
retain the report verification.

The next small repair candidates are colour-picker clipping and actual thumbnail
preview-checkbox input/enabled behavior. Their proposals remain unapplied at this
checkpoint. Other pending candidates, including thumbnail Debug clear-action
evidence and idle Options clipping, receive no new credit. Validate and publish
each subsequent batch before accumulating more implementation work.
