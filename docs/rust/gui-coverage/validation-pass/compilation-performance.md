# GUI compilation investigation

This is a bounded build-performance follow-up to the validation pass. It does
not change feature statuses or establish feature completion. The source baseline
is `7d86cdca4aaabdf7f37020418ae75292950ad4d6`, whose [full CI run](https://github.com/jkendall327/hydrus-mine/actions/runs/37405811109)
was allowed to finish. Logs and 250 native render images were preserved before
starting isolated local measurements. The run failed; its images alone are not
validation approval.

## Completed hosted baseline

These are Actions step durations, not a breakdown of compiler phases. Jobs run
concurrently, so adding platform durations does not give workflow elapsed time.

| Lane | Step | Duration | Result |
| --- | --- | --- | --- |
| Linux | Compiler diagnostics | 16m48s | Passed |
| Linux | Strict Clippy | 11m17s | Failed |
| Linux | Workspace tests | 26m10s | Failed |
| Reference/backend | Tests | 3m20s | Passed |
| Windows | Workspace/all-target build | 33m14s | Passed |
| Windows | Lifetime replay | 5m00s | Timed out compiling; no test executed |
| Windows | Workspace tests | — | Skipped after timeout |
| macOS | Workspace/all-target build | 38m09s | Passed |
| macOS | Workspace tests | 10m14s | Failed |

## Windows replay

The Windows workspace build already emits the GUI integration-test executable.
The following package-selected Cargo test command started compiling dependencies
again inside the five-minute execution guard. Different feature unification is
a plausible cause, but the old logs do not establish its exact fingerprint cause.

The build now captures Cargo JSON while displaying compiler diagnostics. A
helper selects the exact `hydrus-gui` package ID from Cargo metadata and requires
one `gui` integration-test executable with the test profile. It records the source
commit and executable hash. The unchanged five-minute replay guard invokes that
executable directly with `headless_lifetime:: --nocapture`, from the package
directory, with runtime DLL search paths. A successful exit must also report a
nonempty successful test run. Source/executable mismatches, ambiguous artifacts,
zero tests and nonzero exits fail the guard.

The full workspace test command still follows the replay. Both platform lanes,
strict linting and all existing tests remain. Existing publication step names
and the historical CI evidence contract are retained; Windows proof and replay
logs are uploaded separately for inspection.

Validation so far: 14 focused Python tests pass, including build-failure status,
artifact selection, source/hash checks, runtime environment and zero-test refusal.
A tiny independent Cargo probe confirmed the workspace-build artifact contract.
Hosted execution of the changed guard is still pending; no Windows time saving
is claimed yet.

## Linux compiler and lint diagnostics

The separate workspace `cargo check` pass is removed. All-target strict Clippy
performs type checking and linting before test code generation, retains
`--locked --keep-going` and denied warnings, and uploads diagnostics immediately.
Workspace tests still run after a lint failure, and the platform/reference lanes
and parity ratchet remain required.

The tradeoff is failure feedback: an upstream lint can prevent Clippy from
reaching a dependent target whose type errors a preceding check might have
reported. Independent targets still receive diagnostics, and the subsequent
test compilation provides further compiler feedback. Green validation still
requires every original compiler/lint/test gate covered by Clippy and tests.
The old check's 16m48s is not a measured saving; work formerly cached by it may
move into Clippy. Compare the complete replacement sequence before attributing
elapsed-time improvement.

The evidence collector reads the workflow at the requested source commit.
Historical workflows declaring `compiler diagnostics` still require its success;
the Windows lifetime replay and four publication jobs remain mandatory. Nine
collector tests pass, including historical-step enforcement and rejection of
wrong-source, missing, duplicate or failed required evidence. Clippy artifacts
now contain both compiler and lint diagnostics; they are not labelled as proof
of a separate check command.

## Cache inventory and cleanup

Exact Swatinem cache hits skip saving, even when `cache-workspace-crates` was
enabled after that immutable snapshot was created. This affects existing
platform snapshots; the completed baseline did also save a new 1.51 GiB Linux
snapshot. Retention and Cargo freshness must be checked separately: restoring
an archive does not prove an unchanged library stayed fresh.

After confirming there were no active CI runs, eight cache entries belonging
only to closed/merged PR merge refs 46, 47, 48 and 58 were removed. This reclaimed
5,969,193,330 bytes (5.56 GiB). The subsequent API inventory had eight entries
totaling 6,205,385,932 bytes (5.78 GiB). Branch snapshots, including the new Linux
baseline cache, were retained. This makes room for bounded rolling snapshots;
it is storage housekeeping, not a measured compilation speedup.

## Local measurements and limits

An isolated GUI compilation experiment is measuring Cargo unit times, the Slint
compiler call, compiler/linker resource usage, and cold versus edit rebuilds.
It uses one Cargo job and no competing GUI build. The initial compiled target is
empty, registry downloads are already cached, and incremental compilation is
disabled. These controls model a CI-like build, not an ordinary incremental
developer session. Linker duration is included in rustc elapsed time and must
not be added to it.

The first local attempt failed before GUI compilation after 765 seconds because
the benchmark omitted the existing native-library activation environment. Its
partial timings are preserved and excluded from successful-build comparisons.
Fontconfig header, linker and runtime preflight then passed using the existing
installation, and the corrected cold measurement started with an empty target.

That second attempt completed Slint generation in 6.535 seconds and emitted
86,680,537 bytes / 796,709 newline characters of generated Rust (SHA-256
`89ab5f7a7e6748ea51c3ce3e653e2c3d3946303d86d75af337fceb570c6452ab`).
This is current-source evidence; the older 35 MB sample is not the current size.
The subsequent Rust compilation was stopped after 27m14s by a conservative raw
cgroup-headroom guard at 4.86 GiB sampled process-group RSS. OOM counters stayed
zero; a simultaneous conservative estimate including clean inactive file cache
showed about 8 GiB available. This was a measurement-policy stop, not evidence
of an OOM or a successful cold build. A new empty-target measurement accounts
for clean inactive cache while retaining memory, disk, OOM and wall-clock bounds.

The bounded local results and retained structural changes are recorded below.
Hosted savings and complete revised CI duration remain unmeasured.

## Delivery timebox (2026-10-06)

Further compilation experiments are capped at 06:38:15 UTC on October 6,
following the owner's validated-delivery reassessment at 04:38:15 UTC. Finish
one baseline/UI-extraction comparison where practical; defer the larger edit
matrix and additional cache design. Incomplete measurements must be labelled
as such, not used to extend optimization indefinitely. Then prioritize the
existing repair batch, full validation and publication of qualifying completions.

The latest published checkpoint is commit `215814c2b`, October 5 at 00:05:53 UTC:
240 signed off. At this reassessment, zero were newly signed off in the preceding
24 hours and 135 candidates awaited sign-off. The gap exceeded 28 hours. Failed
strict Clippy/runtime validation and slow GUI compilation are the current
blockers. These are delivery metrics, not inventory-status counts.

## Bounded Linux workspace-cache pilot

Manual full validation can opt in with `workspace_cache=true`; it defaults off.
A separate rolling workspace snapshot uses a compatibility prefix followed by
source/run/attempt, so immutable exact hits do not prevent saving newer work.
The existing dependency cache and its fallback remain separate. Selected
workspace libraries, generated build outputs and fingerprints have a 4 GiB
payload cap (plus inventory metadata); housekeeping retains at most two snapshots
per trusted ref within a 3 GiB compressed namespace budget. Other platform cache
behavior remains unchanged.

Restoring an archive alone did not retain Cargo freshness in a tiny controlled
fresh-checkout probe (0/1 ordinary libraries fresh). The pilot therefore verifies
tracked input bytes/modes and a complete artifact inventory before restoring
input timestamps. It invalidates prior workspace outputs before merging;
partial restoration rolls back timestamps and purges mixed artifacts, blocking
compilation if recovery cannot be verified. Unsupported target layouts use the
ordinary build path. The final helper's positive probe reused the ordinary
library while executing both unit and integration tests. Twenty-one portable
restore-safety regressions pass against the integrated workflow.

These are correctness and tiny-library freshness results, not a hosted GUI
speedup. Seed and reuse observations will accompany necessary repair validation;
no separate open-ended cache experiment is required. Windows/macOS timestamp
restoration is not enabled.

## Experiment stop and generated UI extraction

Further performance experiments ended at 05:11 UTC on October 6, ahead of the
06:38 deadline. No fourth cold attempt or larger edit matrix is planned.

| Measurement | Slint generation | Observed wall time | Sampled process-group peak | Result |
| --- | --- | --- | --- | --- |
| Empty compiled target, corrected environment | 6.518s | 1748.747s (29m09s) | 11,073,675,264 bytes (10.31 GiB) | Stopped by estimated available-memory guard |
| Extracted generated UI, dependencies cached | 6.745s | 195.085s (3m15s) | 10,880,774,144 bytes (10.13 GiB) | Stopped by the same guard |

Both source/instrumentation identities remained stable, cleanup left no surviving
compiler processes, and no OOM was recorded. The baseline did reach the cgroup
memory ceiling and reclamation pressure before estimated available memory fell
below 2 GiB. These are incomplete builds, not successful elapsed-time samples.
The dependency-cache difference prevents comparing their wall times as a speedup.
Neither result establishes lower peak memory, linking cost, callback/test/Slint
rebuild time or a percentage improvement. Cargo logs, partial unit measurements,
resource samples and failure classifications are preserved separately from CI.

The small extraction is retained for full validation: `hydrus-gui-ui` owns the
single generated Slint module, and `hydrus-gui` re-exports the same public types.
Authored UI files and resources retain their locations. The generated-only
library disables its empty unit, bench and doc harnesses; existing controller
and integration tests remain. A Cargo 1.94 control confirmed `bench=false` is
also needed to avoid a duplicate cfg(test) library under all-target checks.
This supports the reuse mechanism, not a measured GUI build-time claim.

Comparing baseline and extracted generated Rust found only ten gettext-domain
strings changed by the new package name (30 bytes). Slint 1.18 hardcodes that
domain from CARGO_PKG_NAME; the build script compiles in a child process with the
original `hydrus-gui` domain, preserving translation lookup without unsafe
process-environment mutation. The child inherits Cargo's paths and forwards its
normal dependency/resource tracking directives. Cargo locked metadata and
formatting checks pass; complete native compilation, runtime rendering and
platform behavior must pass the ensuing full-validation checkpoint.

No additional UI split or paid runner change is proposed. The remaining large
Rust compilation unit is still a bottleneck. Necessary repair CI will measure
the resulting workflow and exercise the optional cache seed/reuse; performance
work will not postpone delivery for another local experiment matrix.

The production build-script child was subsequently compiled with denied Rust
warnings and executed against the repaired UI with Slint warnings denied. It
succeeded, retained all ten original translation-domain strings, emitted the
new measured-frame getters and reported imported UI/static resource dependencies.
This verifies source generation and the child path; full generated-Rust compilation
and cross-platform runtime validation remain CI gates.

## Subsequent platform priority

The owner's later October 6 instruction makes Linux the required publication
platform and defers macOS and Windows. Full Linux strict linting, workspace
tests, reference/backend replays and the parity ratchet remain required.
Secondary-platform jobs and the Windows artifact replay stay available through
`secondary_platforms=true`, with their evidence preserved separately. This is
an explicit delivery-scope decision, not a cross-platform success claim.

The first extracted-UI run, `37417762189` at `04a82acc6`, reached the GUI
integration-test target on Linux and macOS but failed on three misplaced lint
attributes; its parity lane also exposed a Qt/MPV expectation mismatch. These
are repaired separately. Linux saved a rolling workspace snapshot under its
source/run/attempt key even after the failure (651,369,586 compressed bytes).
The subsequent platform-policy workflow edit changes the cache compatibility
key, so the next run must seed that new configuration before reuse can be
observed. Keep this conservative invalidation; no successful before/after timing
claim follows from these failed builds.

## Observations from completed ordinary validation

Full Linux run `37425380545` at `56b93ae49` passed all required checks and
696 GUI tests. Its test-only repair reused the generated UI in both Clippy and
test compilation. The preceding run `37422951212` at `f9bc77be8` changed UI
sources and rebuilt that crate. Cargo-reported phases were:

| Phase | UI-changing run | Test-only repair run |
| --- | ---: | ---: |
| Strict all-target Clippy | 6m18s | 1m29s |
| Test-profile compilation | 6m34s | 4m33s |
| GUI test execution | 129.58s (2 failures) | 216.45s (all passed) |

The rolling cache restored verified workspace artifacts and saved a new
source/run/attempt snapshot. This confirms compiled-UI reuse during test edits,
and records useful compilation savings in normal validation. Different changes
and shared hosted runners prevent treating these observations as an isolated
extraction benchmark or controlled cold-build comparison. Test execution was
slower; no peak-memory or isolated linking improvement is established.
[Logs and measurements](../checkpoints/56b93ae49/validation-outcome.json) are
retained with the checkpoint. The bounded investigation remains closed.

The next ordinary validation run, `37428534955` at publication-only source
`ff858ded1`, passed but exposed a remaining cache limitation. Adding tracked
evidence files changed the whole-repository input path set. The downloaded
workspace snapshot was rejected before mutation with
`input paths added/deleted; reject workspace snapshot`; the conservative guard
then allowed a normal build with dependency fallback. Both profiles compiled
`hydrus-gui-ui`. Clippy took 10m17s and test-profile compilation 12m13s; all
696 GUI tests passed in 221.51s. This is a confirmed cache-reuse limitation for
publication commits that add files, not evidence of an isolated extraction
regression or a fresh Slint-generation timing. No safety guard was weakened and
no further optimization experiment was started.
[Exact logs and observations](../checkpoints/ff858ded1/validation-outcome.json)
are retained with the second checkpoint.

A bounded follow-up after the 334-completion checkpoint adds a dedicated manual
`codex/validation-runner` ref to the existing cache-save and maintenance allowlist.
Per-batch PR refs cannot save rolling workspace artifacts; freezing reviewed PR
heads therefore broke continuity. The dedicated ref can advance only between
completed checkpoints and must identify the same immutable SHA as its PR source.
It is excluded from push triggers to prevent a push run cancelling manual full
validation through the shared concurrency group. It must never be a PR head.

The exact-ref/same-repository/manual/opt-in guards, safe-restore checks, dependency
fallback, two-snapshot ceiling per ref/family and aggregate 3 GiB workspace-cache
cap remain. The 21 existing cache safety tests pass, and the collector's required
Linux workflow contract is unchanged. The first dedicated-ref run will seed its
cache; later actual reuse must be measured before claiming an improvement.
Run `37510853142` missed both caches and took 28m33s for the Clippy step and 26m02s
for test compilation; these are hosted observations, not a controlled benchmark.

A second bounded correction excludes only archived evidence under
`docs/rust/gui-coverage/checkpoints/` and `docs/rust/gui-coverage/audit/` from
the timestamp ledger. These prefixes cover all 53 files added by the latest
publication; source inspection found no Rust, Slint, build-script or xtask
consumer. All tracked entries still pass mode, conflict, duplicate, total-count
and safe-path checks before exclusion. Excluded files retain checkout timestamps;
build inputs and neighbouring coverage paths retain the existing path-set,
content, mode and timestamp guards.

The original 21 safety tests remain, with four additional real-Git-index tests
covering publication additions/changes/deletions, retained-input invalidation,
unsafe excluded paths and rejection of the old ledger version. Ledger version 2
and the existing helper compatibility hash require one fresh cache seed.
Hosted reuse and timing improvement from this correction are not yet established.

The revised-policy run `37540746618` at `e886e68ed` passed. Dependency cache
restored; workspace cache missed as expected for the new compatibility hash and
saved a seed. Clippy compilation took 12m40s, test compilation 12m00s and all
699 GUI tests passed in 189.38s. All 25 cache safety tests passed on hosted Linux.
Maintenance retained 1,379,443,388 bytes under the 3 GiB cap. These are ordinary
validation observations, not a controlled benchmark or proof of workspace reuse
under the new policy. [Evidence](../checkpoints/e886e68ed/validation-outcome.json).

## Runner-image cache miss, 2026-10-07 UTC

Validated source `4b5e7ae15`, run `37551810449`, used Ubuntu image
`20261004.327.1`, following `20260927.320.1` in the preceding run. Pinned rustc
remained 1.94.1. The workspace compatibility key includes the image version and
changed from `af4fa2e32ed1c3da4471dd09` to `d375ff77746ca54e3a64f5c5`;
no compatible snapshot was found. Swatinem also missed: its environment key
includes installed Rust versions, and the preinstalled version changed from
1.98.1 to 1.99.0. The existing caches had not disappeared from the API inventory.

Strict Clippy took 36m11s and test compilation 25m32s; the 700-test GUI suite
then passed in 228.13s. These are run observations, not an isolated speedup or
a Slint-generation/Rust/linking breakdown. Both cache mechanisms saved new
snapshots; bounded maintenance retained 1,146,920,388 workspace-cache bytes.
The full logs and exact-source result are preserved in
[the checkpoint](../checkpoints/4b5e7ae15/README.md). No further optimization
experiment was added to this validation batch.

## Verified cache restore, predicate checkpoint

Source `170ab0525`, full Linux run `37560085258`, used image `20260927.320.1`
and restored its compatible older workspace snapshot from `80bcb531f`:
4,113 tracked-input mtimes and 230 coherent artifact files passed verification.
The dependency cache also hit. All 25 cache safety regressions passed; Cargo
freshness checks remained enabled. The restored snapshot is cache input, not
validation evidence from that older failed source.

Strict Clippy took 5m48s; test compilation took 6m37s and the full test step
12m12s. All 704 GUI tests passed in 135.71s. Clippy still reported compilation
of `hydrus-gui-ui`; these logs do not isolate Slint generation, Rust compilation
or linking. The preceding run used a different image and missed compatible
caches, so this is measured hosted reuse and timing, not a controlled speedup.

The run saved a fresh 293-file workspace snapshot (2,200,055,047 uncompressed
bytes). Maintenance retained 1,379,350,054 compressed workspace-cache bytes,
within the 3 GiB cap. [Exact logs and outcome](../checkpoints/170ab0525/validation-outcome.json)
are retained. No additional optimization experiment was opened.

## Radio checkpoint cache reuse

Source `32d9dbb74`, full Linux run `37564321392`, restored the preceding
`170ab0525` snapshot on the same `20260927.320.1` runner image. Verification
accepted 4,122 tracked-input mtimes and 293 coherent artifact files. Dependency
cache reuse and all 25 cache safety regressions passed. A fresh run-specific
293-file snapshot was saved; maintenance retained 1,379,355,380 compressed bytes
under the 3 GiB cap.

Strict Clippy reported 1m30s (91s for the workflow step), versus the preceding
348s step. Test compilation fell from 397s to 272s. Neither current log reports
compiling `hydrus-gui-ui`. The source change here is test-only, unlike the prior
production change, so these observations are not a controlled speedup claim.

GUI runtime rose from 135.71s to 226.21s. The full test step consequently took
766s versus 732s previously; compilation improvement did not shorten that step.
All 705 GUI and 67 media tests passed. The logs do not isolate generation, Rust
compilation and linking, or establish the cause of runtime variation.
[Exact outcome and logs](../checkpoints/32d9dbb74/validation-outcome.json) are
retained. No further optimization experiment was opened.

## Sort/group checkpoint after runner-image change

Source `7c74c6171`, full Linux run `37567007002`, used runner image
`20261004.327.1`, following `20260927.320.1` in the radio checkpoint.
The dependency cache hit, but no workspace snapshot matched compatibility prefix
`d375ff77746ca54e3a64f5c5`. No workspace artifacts or input mtimes were restored.
All 25 cache safety tests passed. A fresh 293-file snapshot was saved under this
run's key: 2,200,273,639 uncompressed bytes and 345,087,701 compressed bytes.
Bounded maintenance retained 1,379,396,172 bytes, within the 3 GiB cap.

Strict Clippy reported 9m45s (586s for the workflow step), test compilation
11m32s, and the full test step 19m56s. All 706 GUI tests passed in 234.50s;
all 67 media tests passed in 1.01s. The generated UI crate was rebuilt.
This change edits Slint and the runner image also changed, with a workspace
cache miss. These observations cannot attribute the slower build solely to the
Slint edit or separate generation, Rust compilation and linking.
[Exact outcome and logs](../checkpoints/7c74c6171/validation-outcome.json) are
retained. No further optimization experiment was opened.
