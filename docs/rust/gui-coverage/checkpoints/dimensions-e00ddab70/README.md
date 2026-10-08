# Dimensions preset verification

This packet covers exactly nine original historical first-pass leaves: the six
ratio presets and the 1080p, 720p and 4k resolution presets. The implementation
ledger remains at 376. Approval of these nine leaves brings the deduplicated
verified total to 385 of 1,274, leaving 889 without explicit sign-off.

Source `e00ddab707e031bb3a8feaa619dbe18c26a3a6fa` passed full Linux run
[37723417979](https://github.com/jkendall327/hydrus-mine/actions/runs/37723417979).
Strict Clippy and all 2,203 tests passed, including 717 GUI and 67 media tests;
there were zero failures and three existing intentional ignores. Independent
review approved exactly the nine leaves after inspecting all three native PNGs
and the Qt reference. Root render review and inventory-preservation evidence
are retained alongside the exact-source manifest, CI metadata and logs. Offline
browser checks passed at1365x900 and390x844, including the385-item filter,
selected approval details and both inventories, with zero page errors, network
requests or horizontal overflow. Both report captures were directly inspected.

The fresh Qt recording activates the actual preset buttons through the real
FleshOutPredicates modal consumer and records accepted predicates, labels, live
recent history and cancellation. It uses Button.click control signals. The
reference image shows the real dialog before acceptance. The recorder does not
prove pointer hit testing or history durability across a Qt restart.

Native regressions dispatch pointer presses/releases at measured preset centers,
check all nine buttons at normal and narrower sizes, and verify that acceptance
retires the editor and updates its owner's search and recent predicates. Each
case reopens the store independently and compares typed recent history. A retained
retired editor cannot accept again; hidden/cancelled owners and old editors with
a live successor cannot change search or history. Those guard checks invoke
callbacks. Sorted page/history comparisons establish membership and values,
not ordering parity.

Normal and narrow native captures cover the nine preset controls. The accepted
owner capture shows the 1080p width and height predicates. Editable dimension
fields, operators, tolerances and wider editor layout are separately assessed.
No parent, alias, native ID or editable-field completion is claimed.

The new historical verification ledger stores immutable source, CI, manifest,
review and artifact hashes for each approved batch. Report/checklist totals use
the deduplicated union with implementation completions, retaining separate totals.
Regression checks reject deleted approvals, historical rollback, duplicate or
ineligible leaves, altered artifacts and unsupported implementation credit.
Existing reference node assessments and the entire native inventory are retained.

Linux is the required validation platform. Windows and macOS remain deferred
under the exact source's publication policy. The earlier failed source's two
test-setup type errors were corrected before successful validation; failed runs
grant no completion credit.
