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

## Remaining measurements

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

Generated-UI extraction and cache retention/freshness remain under investigation.
Their benefits and the complete revised CI sequence will be reported only after
measurements.

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
