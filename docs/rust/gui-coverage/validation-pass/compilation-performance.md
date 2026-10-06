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

Generated-UI extraction, cache retention/freshness and the separate Linux
check/Clippy passes remain under investigation. Their benefits will be reported
only after measurements. In particular, removing the check step cannot simply
be credited with its entire old duration: Clippy may then perform work previously
cached by check.
