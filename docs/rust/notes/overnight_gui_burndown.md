# Overnight GUI burndown, 2026-10-04

The continuous run implements 80 distinct original reference features to first
pass: 70 previously missing and 10 previously partial. The initial goal was 40;
the integration target was raised to 80. Six GPT 6.1 Sol agents at High worked
in independent branches while the root integrated and pushed checkpoints.

| Reference inventory status | Before | After | Change |
| --- | ---: | ---: | ---: |
| Missing | 862 | 773 | -89 |
| Partial | 378 | 368 | -10 |
| First pass | 572 | 671 | +99 |
| Total original entries | 1,812 | 1,812 | 0 |

The 99 newly first-pass entries include 80 concrete implementations, 13 parent
or alias assessments, and 6 evidence-only reassessments. Only the 80 concrete
implementations count toward the implementation goal. Two formerly missing
concrete features now have partial implementations; two existing partial leaves
have further behavior improvements. Eight total missing entries moved to partial,
including parent groups. The two reference menu placements of “close other
pages” share one behavior and are counted once.

## Implemented behavior

| Area | Concrete completions | Main changes |
| --- | ---: | --- |
| Pages, notebooks and backups | 27 | Six reference sorts, four moves, navigation, close groups, rename/send, duplication and collapse; independent session backups retain all native importer queues. |
| Options with live consumers | 19 | Normal/advanced limits, Options page memory/search placement, exit confirmation, backup retention, close focus, notebook rename preferences, wake grace, hash prefixes and default search tag service. |
| Favourite searches and tag editing | 13 | Favourite filter exchange/CRUD, bulk paste, alternate panels, favourite-search domain/sort/collect/search children, remembered/fixed tag-dialog services. |
| Network management | 10 | Cookie clipboard/Netscape exchange, monthly bandwidth history/delete/filter controls, automatic header questions, GUG and viewer URL display choices. |
| Parser and formula workflows | 5 | Context/static formulas, multiple examples and child propagation, parser/formula URL fetch through the real network engine. Nested/zipper and shared child-editor progress is accounted separately. |
| String/date editing | 5 | Date conversion fields/decode/encode, processor exchange and an Options regex-favourite editor. Regex snippet palettes and last accepted converter preferences improve broader partial editors. |
| System predicates | 1 | Accessible mixed-state MIME groups with recorded selection boundaries. Forced hash cleanup now confirms and freezes its draft; warning presentation remains partial. |

The global Tags > migrate entry now reaches the unrestricted migration editor
and reads the same default-service preferences as tag dialogs. This is an
alternate entrypoint, counted separately from concrete completion work. Network
bandwidth persistence merges engine deltas atomically, preventing concurrent
engines from overwriting one another’s totals. A reference replay also corrected
bulk-close depth so a notebook does not close itself. Hosted runtime checks then
caught an unrelated page-sync mutation from absent collect settings to an empty
collect object; cancellation and undo equality checks now preserve that original
representation. Hash cleanup synchronizes its cached text and rejects edit/accept
callbacks during a pending confirmation before they can alter the visible error.

## Validation and deliverables

The implementation checkpoint `c8c5629afc5fc9171db3e310fa7c90caacc029eb` passed [GitHub Actions](https://github.com/jkendall327/hydrus-mine/actions/runs/37192318250) on Linux, macOS and Windows. Linux ran formatting, strict workspace Clippy, workspace tests and the parity ratchet; macOS and Windows ran workspace builds and tests. The native GUI suite contains 270 integration tests. New reference recordings identified in the author manifests were executed separately; reused historical evidence retains its original inspection scope. The inventories pin the tested code commit independently of this documentation follow-up.

The implementation is in [PR #34](https://github.com/jkendall327/hydrus-mine/pull/34), stacked on the existing GUI-map PR #33. This report and the updated inventories/HTML are published in a docs-only follow-up stacked on PR #34; no merge was performed.

The map retains all original reference IDs and now includes 1,295 native entries
covering all 70 exported windows. Every new window has assessed nested controls
or shared-editor navigation. Structure checks found no opaque native windows,
unassessed catch-all nodes or weak first-pass evidence. Changed/ambiguous source
anchors were individually reviewed before repinning; line and module hashes
remain enforced.

The generated HTML is self-contained and needs no server or runtime network
requests. A Chromium check exercised search, view switching and desktop/mobile
layouts without page errors or horizontal overflow. “Completed this run” shows
exactly the 80 goal-counted leaves with their ancestor paths. Use
[gui-progress.html](../gui-progress.html); the frozen baseline, implementation
claims, reviewed patch and exact accounting are in
[gui-coverage/overnight](../gui-coverage/overnight).

## Remaining work and coordination

File Search and Tag Editing options pages remain partial: only their explicitly
implemented service preferences are editable. Automatic date parsing supports a
bounded common-date grammar; full fuzzy/multilingual reference parsing is still
partial. Session backups cover native snapshots and queues, while legacy backup
migration and reference autosave scheduling remain absent. Subsidiary editing,
login management, migration archives/hash conversion and richer tag autocomplete
are still open. Individual hierarchy nodes retain narrower limitations.

Work began at 06:11:45 UTC. The first combined feature set was integrated at 07:24:10 UTC, about 72 minutes later. The handoff was prepared at 10:02:16 UTC, 3 hours 50 minutes after the start. Most subsequent elapsed time was compiler/lint/runtime repair and hosted CI feedback; documentation and coverage work continued while CI ran.

Parallel authoring was useful for these distinct areas. Compiler and lint repair
rounds became the bottleneck after integration; asynchronous CI cannot remove
that dependency. These included newly authored GUI tests using older APIs and
component handles whose owners had already dropped, alongside strict style
diagnostics. Linux eventually passed the full all-targets type/lint check before
starting test code generation. Runtime failures also exposed test setups that
assumed newly created notebooks were empty, imported network traffic was active,
or basic-fixture files had no existing URLs. Those setups now name stable page
identities and isolate their inputs; cancellation equality checks and the original
fetch/viewer timeout limits remain in place. One duplicate test now persists a
direct tab removal before checking storage cleanup, and verifies the copied
media survives exactly after reopening.

The final Windows checkpoint exposed a platform-dependent loopback fixture:
accepted sockets could inherit the listener's nonblocking mode and close before
the client wrote its headers. That triggered the engine's 15-second connection
retry, exceeding the test's eight-second deadline. Accepted sockets now explicitly
use blocking reads. The original fetch deadline and assertions remain unchanged.

I would keep parallel authoring for independent domains. Integrate backend/model
changes as they arrive and exercise the combined GUI at checkpoints. Have
authors check existing APIs and baseline fixture state before handing off GUI
regressions, since feedback arrives much later for those large test binaries.
Collecting all workspace failures in one Linux CI attempt (`--no-fail-fast`) and
retaining generated GUI render PNGs as CI artifacts would also shorten later
repair and visual-review work. The configured `rust-cache` excludes workspace
crates: the final pre-fix Windows checkpoint restored a 674 MB dependency cache
but still spent 12 minutes 31 seconds in the workspace build. Evaluate selective
workspace-artifact caching against repository storage limits before another
long run; a restored dependency cache does not eliminate the large GUI rebuild.
On the final Linux run, Clippy took 4 minutes 29 seconds and the test build took
10 minutes 5 seconds; executing the 270-test GUI suite itself took about 39 seconds.
Keep the final workspace run. There is no
measured serial baseline, so this run cannot support an exact minutes-saved claim.

Local builds and mutation tests were omitted as requested. Hosted CI provided the
heavy checks while source work continued. The CI workflow retains failed-build
caches and allows this overnight branch's checkpoints to validate independently;
it skips duplicate push jobs when the same checkpoint has a pull-request run.
Free disk remained about 8.5 GiB out of 32 GiB during the run; the existing target
directory stayed about 20 GiB, and the six new worktrees used about 0.9 GiB in
total. After successful CI, their clean checkouts were retired while preserving
all branches; free space is now about 9.4 GiB. Git pushes and GitHub connector CI/PR access
worked. The installed Playwright package lacks its matching bundled browser;
the system Chromium works with the environment’s socket/network permission.
