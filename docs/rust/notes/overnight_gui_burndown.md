# Continuous GUI parity implementation, 2026-10-04–05

The current validated milestone is **240 distinct original reference leaves**: **222 formerly Missing and 18 formerly Partial**. This adds 51 implementations to the prior 189-item milestone. Counts use the frozen 1,812 original IDs and exclude parents, aliases, evidence-only reassessments and new native IDs. A separate 28 Partial improvement claims earn no additional completions.

| Original reference status | Frozen baseline | Validated at 240 | Change |
| --- | ---: | ---: | ---: |
| Missing | 862 | 581 | -281 |
| Partial | 378 | 390 | +12 |
| First Pass | 572 | 841 | +269 |
| Total original entries | 1,812 | 1,812 | 0 |

Raw status changes also include structural assessments. Partial increases when a previously missing broader feature gains a working slice. The concrete completion ledger remains the measure of implemented original leaves.

## Validated 240-item checkpoint

Source `028fd72f4a3cb747e7588d0987bbb99dd83d53f7` passed all four required jobs in [workflow 37242969868](https://github.com/jkendall327/hydrus-mine/actions/runs/37242969868): Linux formatting, strict Clippy, workspace tests and parity ratchet; macOS and Windows builds/tests; and the separate reference/backend lane. **455 GUI integration tests passed on each operating system.** Unix model/store suites passed 323/171 cases; Windows passed 322/170 because of platform-gated cases. Two store cases remain ignored on each platform. The network/login suite passed 14 cases on each platform. Exact job, step, log-hash and suite evidence is in [the CI record](../gui-coverage/overnight/ci-evidence.json) and [validation summary](../gui-coverage/overnight/validation-summary-028fd72f.json).

The additional 51 completions cover command palette controls (12), filename tagging (2), migration (5), viewing statistics (6), millisecond viewtime (1), Files/Trash (6), OR controls (4), Manage Tags (2), frame flip/reset (4), tag-banner launch/editor controls (6), local-service bulk actions (2), and deleted-record clearing (1). Broad clipboard/Undo, Manage Tags, service-review, frame and banner families retain their Partial boundaries.

The canonical map has **1,733 native entries across all 97 exported Window components**. The source census and anchor checks found no opaque/unassessed windows, catch-all entries or weak First pass rationales. [The offline HTML](../gui-progress.html) embeds the inventories; its completion filter contains exactly 240 original leaves. Root checked its metrics, search/reset, both tabs, keyboard/shared navigation and desktop/mobile layouts, with no page errors, runtime network requests or horizontal overflow. System Chromium blocks `file://` navigation by administrator policy, so this check loaded the exact file contents offline with `set_content`; it does not claim successful file navigation. [Browser evidence](../gui-coverage/overnight/browser-evidence.json) records the checked HTML hash and policy limitation.

Root reviewed eight native PNGs from exact-source [artifact 11319130943](https://github.com/jkendall327/hydrus-mine/actions/runs/37242969868/artifacts/11319130943), covering palette, OR validation, filename rules, deleted-record questions, banner controls/thumbnails, paused migration and viewing statistics. [Visual provenance](../gui-coverage/audit/visual-review-028fd72f-follow-through.json) names those images and findings; this is not an all-window pixel review. Reference recordings and historical renders remain separate evidence. No local Cargo builds, Rust tests or mutation tests were run for publication.

Hosted jobs occupied 113.9 minutes in total and overlapped within 40.7 minutes. With the same observed durations, sequential execution would add about 73.2 minutes. This measures CI overlap, not total authoring time saved. Parallel implementation continued while those jobs ran. The next notebook/viewer/rating and nested-editor changes remain uncounted until their own source review and hosted validation pass.

## Historical 189-item milestone

The prior validated milestone was **189 distinct original reference leaves**: 80 from the prior validated checkpoint plus 109 additional implementations. The initial goal was 40 and the integration target was raised to 80; that milestone was already achieved. Of these, **174 were Missing and 15 were Partial** in the original inventory. Cumulative progress keeps the frozen 1,812-entry denominator and excludes parent groups, aliases, evidence-only reassessments and new native inventory IDs.

| Original reference status | Frozen baseline | Validated at 189 | Change |
| --- | ---: | ---: | ---: |
| Missing | 862 | 647 | -215 |
| Partial | 378 | 375 | -3 |
| First Pass | 572 | 790 | +218 |
| Total original entries | 1,812 | 1,812 | 0 |

| Implementation manifest | Cumulative counted completions |
| --- | ---: |
| bandwidth-history | 3 |
| cookies | 3 |
| downloader-display | 3 |
| export-default-directory | 1 |
| favourite-searches | 4 |
| gallery-source | 1 |
| header-approval | 1 |
| import-options | 1 |
| integration | 1 |
| login-workflows | 3 |
| media-background | 4 |
| media-closing | 4 |
| media-cursor | 1 |
| media-focus | 2 |
| media-hovers | 4 |
| media-options | 6 |
| media-pointer | 2 |
| media-zoom-loop | 2 |
| network-controls | 15 |
| options | 19 |
| pages | 27 |
| parser-children | 5 |
| parser-workflows | 5 |
| search-options | 8 |
| sessions | 32 |
| sibling-connector | 1 |
| string-dates | 5 |
| subscription-concurrency | 1 |
| subscription-failure-limit | 1 |
| tag-autocomplete | 8 |
| tag-dialog-defaults | 2 |
| tag-dialog-preferences | 4 |
| tag-filters | 7 |
| tag-list-display-types | 2 |
| unselected-tag-cap | 1 |

The previous checkpoint `c8c5629afc5fc9171db3e310fa7c90caacc029eb` is independently recorded in the ledger with its prior CI evidence. The current source checkpoint `a1a997cd3a64232b65f0b1b1bad16499487af79e` passed [GitHub Actions](https://github.com/jkendall327/hydrus-mine/actions/runs/37216336747): Linux formatting, strict workspace Clippy, workspace tests and parity ratchet; macOS and Windows workspace builds/tests; and the parity-models reference/backend lane. New reference recording execution is declared separately in the author manifests. Historical source-only assessments retain their original scope. No local Cargo builds or mutation tests were run for this publication.

The canonical inventories pin this tested source commit independently of the documentation publication commit. All original reference IDs are preserved. The native census contains 1,622 entries and all 89 exported Slint Window components, with assessed child/control or shared-editor navigation. Source-anchor remapping and line/module fingerprints passed; no opaque native windows, unassessed nodes, catch-all nodes or weak first-pass assessments remain.

Subscription import-options clipboard actions map to `subscriptions-copy-options`; the separate serialized-subscription exchange entry `subscriptions-exchange` remains Missing in this validated checkpoint. Remaining per-node limitations, broader Partial behavior and shared editor boundaries remain in the inventories. First pass denotes the explicitly assessed scope, not whole-client parity.

[The offline GUI map](../gui-progress.html) embeds both inventories and needs no server. “Completed this run” uses exactly the 189 counted original leaves. [The cumulative ledger](../gui-coverage/overnight/progress.json), [replayable patch](../gui-coverage/overnight/reviewed-patch.json), [exact CI evidence](../gui-coverage/overnight/ci-evidence.json) and source-anchor audit preserve the accounting and provenance. The HTML was rendered with the checkpoint's coverage tool and template. The root checked it in system Chromium: inventory counts, the exact 189-feature completion filter, search, both inventory tabs, reset, and desktop/mobile layouts passed with no page errors, runtime network requests or horizontal overflow. [Browser evidence](../gui-coverage/overnight/browser-evidence.json) pins the checked HTML hash.

## Final checkpoint verification

All four required jobs completed successfully at `a1a997cd3a64232b65f0b1b1bad16499487af79e`, workflow [37216336747](https://github.com/jkendall327/hydrus-mine/actions/runs/37216336747). The 389 GUI integration tests passed independently on Linux, macOS and Windows.

| Job | Conclusion | Test scope | Reported suite duration | Log-derived job span |
| --- | --- | --- | --- | --- |
| [parity-models](https://github.com/jkendall327/hydrus-mine/actions/runs/37216336747/job/111477500410) | success | 272 model cases plus the backend suites | 6.23 s (model suite) | 217.827 s |
| [other-platforms (windows-latest)](https://github.com/jkendall327/hydrus-mine/actions/runs/37216336747/job/111477500492) | success | 389 passed, 0 failed, 0 ignored | 185.28 s | 2186.930 s |
| [check](https://github.com/jkendall327/hydrus-mine/actions/runs/37216336747/job/111477500493) | success | 389 passed, 0 failed, 0 ignored | 65.04 s | 1502.143 s |
| [other-platforms (macos-latest)](https://github.com/jkendall327/hydrus-mine/actions/runs/37216336747/job/111477500514) | success | 389 passed, 0 failed, 0 ignored | 67.02 s | 1354.285 s |

| Job | Step | Log-derived interval |
| --- | --- | --- |
| parity-models | reference and backend tests | 145.514 s |
| other-platforms (windows-latest) | build | 1470.517 s |
| other-platforms (windows-latest) | test | 634.434 s |
| check | fmt | 4.012 s |
| check | clippy | 401.038 s |
| check | test | 1038.173 s |
| check | parity ratchet | 1.173 s |
| other-platforms (macos-latest) | build | 1009.224 s |
| other-platforms (macos-latest) | test | 310.243 s |

Whole-job spans use the first and last timestamped log records. Step intervals run from the command group marker to the following step or cleanup marker and include transition overhead. Suite durations are reported by the test harness. These are log-derived timings, not scheduler metadata or measured serial time savings. The exact CI evidence contains the timestamps, step conclusions and artifact source hashes.

## Visual review provenance

The root agent reviewed stored PNGs from the earlier source `b6e3fab19d586e691dd958740d50a7fbb863a69c`, [workflow 37214881974](https://github.com/jkendall327/hydrus-mine/actions/runs/37214881974), [screenshot artifact 11308751809](https://github.com/jkendall327/hydrus-mine/actions/runs/37214881974/artifacts/11308751809). The review confirmed one namespace-editor button in the Options sort cog, only path/Browse controls in Exporting, the active JSON-name editor rendering, and the subscription-concurrency row rendering. This visual review verifies the observed control collision and blank-render fixes at that source. It is distinct from the final checkpoint's runtime/CI validation and from author-declared reference recording execution. [The visual provenance record](../gui-coverage/audit/visual-review-b6e3fab1-continuation.json) preserves this distinction.

## What changed during the continuation

The later work adds real consumers for search placement and floating autocomplete,
write-autocomplete favourites/children, tag-dialog presentation and independently
captured sidebar/viewer tag display types. Viewer preferences now reach hover
panels, painted information, transparency, zoom switching, animation loop limits,
cursor hiding, focus and close behavior. Session work covers further notebook and
chooser preferences, import-option profiles, autosave controls and editable
namespace sort schemes. Primary and fallback sorts have their own tag-service
cog menus; those two structural menu nodes are excluded from the concrete count.

Downloader work adds subsidiary parser editing, JSON name matching and recorded
timestamp metadata behavior, default gallery/export destinations, subscription
import-option clipboard actions, bounded concurrent subscription execution and
recorded failure-stop handling. Network editors gain matcher favourites, URL
preview, domain-component editing, parser selection, auto-fill review and runtime
job controls. Login credential/result editors are implemented, while broader
login scripts and exchange retain their explicitly recorded Partial boundaries.
Manual export gains tag editing and filename examples; its broader sidebar node
remains Partial because scoped tag regeneration is still absent.

## Coordination and elapsed time

Six GPT 6.1 Sol agents at High authored features in separate worktrees. The root
integrated their commits, resolved shared-file changes and pushed checkpoints.
Backend/reference suites ran in a separate GitHub job while the combined native
GUI compiled on Linux, macOS and Windows. Reference recordings ran locally;
heavy Cargo builds and mutation tests did not. No mutation-test defect count is
claimed because no mutation tests were run.

Work began at 06:11:45 UTC on October 4 (23:11:45 PDT on October 3). The first
combined feature set was integrated about 72 minutes later; the first 80-item
handoff was prepared about 3 hours 50 minutes after the start. Feature work
continued beyond that milestone. The 189-item ledger was finalized at 16:58:24 UTC, about 10 hours 47 minutes after the start. The combined GUI integration target contains 389 tests;
the final successful checkpoint runs those alongside the workspace suites. These are elapsed times, not a measured comparison with
serial implementation.

For the final checkpoint, the four hosted jobs occupied 87.7 minutes in total
and overlapped within a 36.4-minute window. Running those same jobs sequentially
with unchanged durations would add about 51.2 minutes. This comparison uses
first/last timestamps in completed job logs; it measures the benefit of
overlapping these CI jobs, rather than total authoring time saved or scheduler
queue time.

Parallel authoring helped cover independent domains. Integration and hosted GUI
compilation became the bottleneck. Several failures revealed actual behavior
issues, including OR-only searches failing to open a page, importer focus not
following the saved preference, and imported subsidiary parser selections not
being highlighted in the queue. Floating search results also depended on
deferred copies of their initial layout geometry; they now use live bindings. Many others were authored regression problems:
stale fixture assumptions, callbacks that did not exist, borrowed component
handles crossing callbacks, missing owned values in threaded test closures, and
strict lint diagnostics. Those repair rounds consumed substantial elapsed time. Visual review of stored CI
frames also caught a shared Options control identifier: namespace sorting and
export-directory editing rendered both controls on either row. Their identifiers
are now distinct. A black sidecar screenshot came from a retired text-entry
window; the render driver now captures the still-open JSON editor and requires
visible pixels.
The report does not equate every CI failure with a product defect. An older
Windows checkpoint also exposed the fake-daemon fixture polling and briefly
winning the serving lock during child startup. Its parent now waits for a
readiness marker written after the child holds the real lock, then asserts lock
ownership; the original deadlines and stop behavior checks are retained.

I would keep parallel authoring, integrate backend/model slices as they arrive,
and validate the combined GUI at bounded checkpoints. Testing every GUI slice
sequentially would repeatedly rebuild the large binary; postponing all checks
until a much larger batch leaves too many failures to untangle. The workflow now
keeps all workspace failures in one run, preserves failed-build caches, exports
Clippy diagnostics before test compilation finishes and retains GUI render PNGs
for visual review. The Linux cache includes workspace crates as a bounded
experiment. The cached Slint source checker also now surfaces and rejects
compiler warnings; it previously hid warnings on successful compilation.
Those changes address observed feedback delays; this run does not
establish an exact time saving from them.

## Environment notes

Git pushes and the GitHub connector's CI/PR APIs worked. The installed `gh`
credential could push but a direct check-runs API request returned Forbidden;
the connector provided the required CI access. Cargo is available after sourcing
the environment activation script; putting its binaries on the default shell
PATH would remove that small setup trap for future agents. Job log downloads were only
available after the full job completed, which motivated the immediate Clippy
artifact. The environment has about 8.2 GiB free on a 32 GiB filesystem; the
existing Cargo target directory remains about 20 GiB. Feature worktrees were
kept as small source checkouts. The system Chromium can check the offline map;
the installed Playwright package lacks its matching bundled browser.
