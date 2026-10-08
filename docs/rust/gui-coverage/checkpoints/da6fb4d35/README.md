# Validated manual export successful-prefix cleanup

Source `da6fb4d3588c514e5cfe28869589379c2cbaaf89`; full Linux run
[37712768495](https://github.com/jkendall327/hydrus-mine/actions/runs/37712768495).
Strict Clippy and all 2,201 workspace tests passed, including 715 GUI and 67
media tests, with zero failures and three existing intentional ignores. Both
actual export frames and the Qt post-failure panel were directly inspected.
Independent final review and report checks are retained alongside this packet.

The selected leaf is `audit-network-export-worker`. All 375 prior sign-offs and
all unselected assessments remain unchanged. The Partial manual-export
parent receives only a narrative correction for its stale failure policy, with
no status promotion or completion credit. This is not a promotion of the
manual-export parent, filename examples/tag-sidebar, or export-and-close lifecycle.

When a file or sidecar fails, confirmed trash-after-export moves only complete
predecessors to trash. Cancellation before deletion—including after the final
copy—suppresses all trashing. Sidecars run before copies and deletion uses 64-file
transactions. A failed later cleanup transaction retains earlier committed counts
and both the original export and cleanup errors; the owner refreshes and retains
failure details. Archived-file physical-delete lock does not block this move to
trash through the combined-local domain.

Six real reference cases use fresh private basic databases, real MirrorFile,
actual sidecar routers, real JobStatus cancellation, synchronous content updates
and SQLite membership after clean shutdown. Worker scheduling and dialog replies
are scripted; copy observation fixes cancellation boundaries without replacing
copying. The reference screenshot shows the actual post-failure panel; the error
dialog was intercepted/recorded and is not pictured. The sandbox's unrelated
Client API listener could not bind; no API requests are needed for this replay.

Native regressions replay the durable membership/copy/cancellation/error results,
exercise a 66-file plan with 65 successful exports and a failed second cleanup
transaction, and drive the actual window's confirmation, asynchronous worker,
refresh callback, retained error and dismissal. Existing atomic-copy, hardlink,
symlink and sidecar regressions remain.

Native path/source preflight is stricter and can reject before writing sidecars;
Qt looks up a missing source after its routers. Native failed/cancelled
Export-and-close remains open for review. These are disclosed differences, not
claims of identical filesystem artifacts or complete Qt window behavior.
Qt blocks on its critical-error dialog before cleanup and can observe
cancellation during that acknowledgement pause. Native instead displays inline
failure details and can begin already-confirmed cleanup immediately. The recorder
intercepts the error dialog, so it does not prove acknowledgement timing parity.
Windows/macOS remain deferred. Historical first-pass inventory entries receive
no implicit signoff.

Publication brings the implementation ledger to 376, with zero pending candidates;
898 of 1,274 goal leaves remain outside explicit sign-off. Historical verification
credit is not added by this checkpoint. The next target is at least 800 verified
goal leaves, with implementation and historical verification kept distinct.
