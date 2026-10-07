# Validated sidebar tag-display action

Validated source `9f6397368d2ecdfcb28e7c7159a8bf195e7d7012`, full Linux run
[37577340514](https://github.com/jkendall327/hydrus-mine/actions/runs/37577340514).
Exactly the namespace-sort ADVANCED tag-display action gains sign-off:
**355 signed off, 20 existing candidates pending**. All 707 GUI and 67
media tests passed. The entire original test file, its two tests and 25
assertions remain; an appended test brings the total to 88 assertions across
three tests. No production or Slint changes belong to this follow-up.

Actual right click leaves the cog closed and all full contexts/media/settings
unchanged. Actual left click refreshes the root; keyboard selection changes modes
2 to 1 to 3 to 2. Reopening verifies live checks and exports three real submenu
states plus the actual root. Root and independent review inspected all four
fresh defining captures. Retained View actions cannot mutate a stale opening,
hidden owner or switched page. Enabled save-default policy preserves complete
settings and reaches a new page; disabled policy leaves settings unchanged.
Explicit cleanup after hide/page switching does not claim automatic popup
teardown or permanent retirement on temporary hide.

The earlier full run at `393c12553` passed 706 existing GUI tests but failed
its new test before the defining captures: a restored input order was compared
with already-sorted reference output before triggering a sort. The setup repair
adds the ordinary sort consumer and an exact sorted baseline while preserving
all 87 earlier assertions. The failed run's logs, artifact metadata and diagnosis
remain in `failed-runs/393c12553/` and `preparation/repair1/`. It earns no credit;
the successful current-source validation above supplies this sign-off.

The unchanged actual Qt rerun reproduces all 16 sort cases, eight collect cases
and mouse routes. All twelve namespace cases have the same media order across
display modes within each service: exact replay and typed contexts prove recorded
routing/preservation, without discriminating display-filter effects. Actual Qt
menu images are painted QMenu grabs with intercepted popup presentation; no
displayed-popup placement, pixel parity or physical OS behavior is claimed.
Fonts, spacing and menu transport differ. No service/collect/DefaultCollect,
parent, alias, remote repository or broader filter-backend credit is added.
Windows/macOS are deferred. The optional sandbox-denied Qt API listener is
outside the successful direct Qt/DB rerun and earns no connectivity claim.

Exact CI logs, independent review, root inspection, native manifest, selected
patch and preservation audit are retained here. Desktop/narrow report checks
passed; prior 354 and all unselected assessments remain intact. The 1,274-leaf
goal continues.
