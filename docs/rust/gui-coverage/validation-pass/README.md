# GUI validation and delivery pass

The pass started from merged source `3702cdbee99473cfd7af073aa044fe847f78f405`.
The prior checkpoint validated `028fd72f` (240 original leaves).

The latest checkpoint validates `7c74c6171` and publishes **351 signed-off
original leaves: 4 new in this batch, 111 across thirteen recent checkpoints,
24 candidates still pending**. Full Linux validation passed, including all 706
native GUI tests and 67 media tests. Exactly two Manage Tags default-sort controls
and namespace-grouping Add/Edit received fresh independent behavioral/rendered
review. Windows/macOS remain deferred.
[Durable checkpoint evidence](../checkpoints/7c74c6171/README.md) retains the
defining native images, actual geometry/scrolling tests, logs and Qt reruns.
The current goal is all individual feature leaves implemented and verified;
historical numeric goals and inventory first-pass counts are not completion
criteria. The next bounded cohort is the three viewer-eye menu candidates.
The notes below preserve the chronological investigation and failed-run history.
Source review of all 153 parity manifests is recorded in `review-1.md` through
`review-6.md`; these reports describe the initial source, not later repairs.
The six `demotions-*.json` files record explicit credit decisions. They exclude
previously validated IDs. Structural parents may retain a scoped assessment but
receive no completed-leaf credit.
The supplemental `demotions-render-evidence.json` defers seven further claims
whose defining native behavior lacks assertions, initially leaving 135 candidates.
One hundred and eleven are now published; 24 remain unresolved. No pending candidate receives
completion credit. Favourites was withheld from the preceding batch because its
capture disagreed with the selected service; actual dropdown input and fresh
review resolved that blocker in the preceding checkpoint.

The next 22-candidate validation at publication source `4b7455ce1` passed all
696 GUI tests but failed one media subprocess test before its PID marker was
observed. The [failed-run evidence](failed-runs/4b7455ce1/README.md) preserves
the exact outcome. Test transport hardening retains all timing values and
assertions and leaves production behavior unchanged. Repaired source `6b66240c1`
passed full Linux validation, including all 67 media unit tests; 21 candidates
then passed fresh review and were published. Favourites stayed deferred at that checkpoint. The
original subprocess failure's precise cause was not recorded, so no particular
spawn error is claimed as established.

The next bounded cohort of 18 existing candidates is now published: 12 affected
by the shared Speed and Memory layout, favourites, and five export/consumer
claims. Intrinsic numeric-row sizing and real favourites dropdown input passed
full Linux run `37447108755`, with 697 GUI and 67 media unit passes. Five independent
review groups inspected fresh images; the staged report passed desktop/narrow
Chromium checks. Six prior wording errors were corrected with zero new credit,
while archived reviews retain their original hashes. The thumbnail Debug clear
claim remains deferred; the next small repairs target colour-picker clipping
and actual thumbnail preview-checkbox input/enabled behavior.

The initial full hosted run is
https://github.com/jkendall327/hydrus-mine/actions/runs/37396014073.
No result from a different source commit establishes validation of the final
repair batch. Native execution, platform validation and rendered review are
required before publication. Mutation testing remains disabled.
The initial Linux runner shut down during Clippy with exit 143; it did not save
Clippy/test/render artifacts or its cache. The retry bounds Clippy to one build
job to avoid concurrent generated GUI metadata competing for memory.

## Repairs under validation

- Backup cancellation returns an incomplete result and preserves the last
  successful-backup timestamp. Source-copy failures during restore preserve the
  existing database, sidecars, media and request. Startup locks the GUI before
  restoring; same-directory aliases and active serving processes are refused.
- Granularity migration persists the physical mover's actual destination, and
  journals exact moves for rollback after cancellation or publication failure.
  It refuses destination collisions and handles non-ASCII unrelated folders.
- Failed auto-resolution decisions refresh pending pairs from the database,
  retain unprocessed rows and report the error. The delayed-popup timing claim
  remains Partial because publication is still checked between chunks.
- Cancelling the shutdown-maintenance question abandons exit without registering
  maintenance. No and timed auto-no remain distinct and continue exit. The
  question owns its timer and rejects retired callbacks.
- Retained network-error windows close only themselves, preserving successors.
- Database maintenance and vacuum confirmations have explicit owner retirement,
  live/pending/once-only admission and native cancellation regressions. Password
  callbacks require the current text/clear step and cannot revive a retired owner.
- Immediate thumbnail maintenance filters the captured selected IDs before
  batching, so unrelated queued files cannot consume its budget. Store/importer
  regressions cover a backlog larger than one batch, exact follow-up jobs,
  reopening, lease contention, cancellation and work budgets.
- Media-view editor and Add chooser callbacks retire on cancellation/replacement;
  Options Apply waits for the child editor. New-maintenance confirmations retire
  with their owning review, including work queued behind a busy writer.

Focused local store validation: nine backup/restore tests, six granularity tests,
the shortcut-import policy regression, the selected-file Store regression, two
selected-file importer regressions, 13 menu tests, the retired popup producer
regression, and strict all-target store Clippy.
GUI regressions are authored for hosted execution; this list is not a successful
full-validation declaration.

## Initial hosted model failures

The shortcut-import test assumed supported built-in shortcut sets were empty.
The corrected assertion compares the complete imported Settings with only the
two recorded capture preferences changed, retaining all supported bindings.
Other initial failures concern enabled Restart/menu ordering and retired popup
producer ownership; their repaired regressions pass locally without disabling
features or suppressing assertions. The initial native compiler lane also found
42 GUI test errors; corrections are being prepared against current production
APIs before the next full hosted source checkpoint.

The first repair commit corrects all 42 reported sites, preserves imported
shortcut defaults in capture tests, and explicitly isolates confirmed-close
tests from the separate shutdown-maintenance prompt. Native recheck is pending.
Existing tests now export additional representative UI images. The sibling-colour
reference replay matches the entire checked-in JSON and recovers four Qt renders;
their native comparison is still pending.

## Remaining boundaries

Demoted controls remain useful Partial implementations. Stored-only background
work policies, deferred-delete/tag-sync switches and unsupported repair/debug
commands do not count as completed. Areas lacking actual reference/native
boundary assertions also remain Partial, even if model tests pass. See each
review and manifest's `validation_review` for the specific reason.

Migration rollback is in-memory, not crash recovery. Restore media mirroring and
final database/sidecar replacement are not one atomic transaction. Both retain
explicit backup/manual-recovery limitations in `docs/rust/DIFFERENCES.md`.

## Current delivery priority (2026-10-06)

The owner has paused broad feature work until this repair batch produces a
published validated checkpoint. The latest publication is `215814c2b` at
2026-10-05 00:05:53 UTC, validating source `028fd72f`: 240 signed off. At the
October 6 04:38 UTC reassessment, zero were newly signed off in 24 hours and
135 candidates awaited sign-off. The checkpoint gap exceeded 28 hours. Inventory
status totals are separate and cannot stand in for these delivery metrics.

The bounded compilation follow-up ends by 06:38:15 UTC on October 6; see
[its measurements and limitations](compilation-performance.md). Existing runtime
repairs are prepared concurrently with the isolated benchmark. Each subsequent
small batch must receive full validation and publication before broad new work.
A further 24-hour publication gap requires an explicit batch/approach reassessment.

The completed `7d86cdca4` run failed strict Clippy and runtime tests. Current
repairs retain exact assertions while correcting stale test entrypoints,
platform shortcut display and completed-exit maintenance preconditions. Hidden
or retired Options numeric callbacks now reject edits before changing a visible
draft; a fixture-derived FFmpeg regression covers the draft, save/reopen and
live timeout reader. Linux also enables existing interchange render exports.
These changes still require native execution and exact-source full CI; none is
credited as a new completion here.

Additional repairs align MPV video zoom defaults with the recorded MPV branch,
match preview fixture metadata to its injected raster, select the recorded file
search domain, and drive current shortcut-edit entrypoints. Menu expectations
retain the recorded implemented maintenance submenus. Unknown URL handoff checks
all seeded associations exactly once. Delayed popup checks wait for the real
refresh timer while retaining exact durable deadlines/content. Importer ownership
is now asserted independently of retained GUI callbacks, without changing any
FFmpeg deadline or kill/reap assertion.

Lifetime tests explicitly establish component destruction before exercising
retired callbacks. The predicate-editor callback-cycle test releases Slint's
shown-window retention before its final Weak assertion. This does not establish
automatic closure of a shown child when its parent is destroyed; that boundary
remains a publication gap requiring separate evidence or repair.

Widget replay repairs establish measured preview geometry, deliver actual tab
changes before reading colour rows, and select the system predicate suggestion
rather than a populated OR summary. Radio focus is checked with real arrow keys
before Return. The palette-only screenshot test disables independent legacy
colour overrides without relaxing its pixel threshold. Manual export exposes
the existing sort controls' measured frames directly, following Manage Tags;
wheel tests continue sending native pointer events and asserting exact results.

Archive/delete readiness and commit now share one precise deadline. An early
millisecond-rounded Slint timer tick rearms for the remaining delay rather than
enabling a button whose commit guard still refuses it. The minimum 1200ms delay,
owner/visibility checks and transaction assertions remain unchanged. Independent
source review confirms timer rearming and ownership; runtime confirmation of the
reported failure remains pending.

The animated WebP repair addresses image-webp 0.2.4's opaque alpha-blending
roundoff. Independent Pillow/libwebp reproduction matches the recorded frames;
applying the dependency's blend formula and ICC conversion reproduces all 192
failed native channels. For conforming declared-opaque animations only, a bounded
reader presents no-blend frame flags, preserving original files, timing and
disposal. Parsing and decoding use the same open file. Alpha-bearing and malformed
container paths are left unchanged. Exact pixel assertions remain; added coverage
checks raw second frames, looping, seeks, flags and existing alpha composition.
No supported fixed dependency release or public blend bypass was found. This is
a narrow workaround, not a general decoder replacement; full native tests remain
pending.

Saved-page and media-sort replays now use actual Refresh instead of entering an
already-present `system:everything` predicate. The reference and native predicate
entry behavior intentionally toggle that predicate off on repeated entry. The
replays retain exact predicates, location, full file membership and existing
selection/wheel/preview ownership assertions; no query-domain or worker behavior
is changed.

The extended sidebar recorder ran successfully against the real Qt client in an
isolated checkout/database on October 6. All ten legacy top-level fixture fields
and the original PNG match exactly. New raw observations show Restore retaining
the accepted hash, rendered raster and original viewing start with no completed
interval; selecting that same file preserves the interval, and final collapse
emits one FinishViewing call. The native assertion now checks this retained state
and preserves its existing exact `(1, 2000)` accounting and selection assertions.

Supplemental actions labelled as a hidden page switch did not change the recorded
current page; they do not prove an actual switch. The observed 568ms FinishViewing
call is below Qt's 5000ms minimum and is not a persisted view. Raw timestamps and
these observations remain intact in the fixture; no fixed native duration is
inferred from them. The separate existing hidden-page tests cover real switching.

The next full run, `37417762189` at `04a82acc6`, exposed a hybrid model
expectation: MPV display labels had been substituted into the older Qt player
recording without changing Qt's different video zoom defaults. The recorder now
captures both actual reference default branches explicitly and checks the macOS
fallback. All original fixture fields remain unchanged. Model replay compares
the complete MPV defaults and both full row lists exactly, without label
substitution. Three ICC geometry lint expectations also move from macro
invocations (where Rust ignores them) to single-assertion blocks; exact float
equality and strict warnings remain. These are validation repairs, not new
feature credit; their full native Linux rerun is still required.

The owner's subsequent October 6 instruction prioritizes Linux, their actual
platform. Current checkpoints require full strict Linux compilation, workspace
tests, reference/backend replays, the parity ratchet and rendered review.
Windows and macOS are deferred and remain opt-in workflow jobs; a Linux
checkpoint does not certify them. Publication tooling checks this policy at the
exact source commit and labels the scope. Historical cross-platform evidence
and earlier completed sign-offs retain their original scope.

The first Linux-only run, `37420034077` at `129c3fd20`, passed strict
workspace Clippy and the reference/backend job. Native GUI runtime reached
685 passes and 11 failures. The follow-up repairs forward stylesheet brush
changes to the existing thumbnail-paint invalidator, hide an exact retired
colour picker on repeated Cancel, and publish initial measured wheel-control
frames through the existing deferred measurement pattern. Regression assertions
are preserved or strengthened.

Replay setup now distinguishes Refresh from predicate toggling, completed exit
from a pending shutdown-maintenance question, and blank-input system editors
from tag searches. The hidden-component lifetime test releases its independent
headless adapter collector before asserting slot/component destruction; it does
not establish automatic closure of a shown child on parent destruction. The
thumbnail menu retains Qt's recorded clear-deletion action, verifies its raw
placement and still compares the complete menu. Slint generation with warnings
denied and formatting pass; full Linux replay of these repairs is pending.
No feature assessments or signed-off totals change.

Run `37422951212` at `f9bc77be8` passed strict Clippy and the
reference/backend job, with 694 native GUI passes and two failures. Nine of the
eleven preceding failures now pass, including the unchanged dark-palette pixel
threshold. Fresh renders and complete logs are preserved. The remaining replays
need Refresh at the final same-owner ICC selection and a genuinely scrollable
Options viewport for the physical wheel-bubbling assertion. Their follow-up
preserves the original consumer, pixel, ownership and scroll assertions.
No new sign-offs are claimed before the next full Linux run and render review.

Run `37443398472` at `115d5532a` passed strict Clippy and parity models but
failed two of 697 GUI tests. Its strict geometry check exposed a 26px row
override around a 30px-minimum spinbox; numeric rows now honor intrinsic height.
The favourites replay bypassed actual ComboBox selection; it now uses measured
pointer and popup keyboard input and preserves its two-second displayed-label
assertion. Fresh full Linux validation and rendered review remain required.
At that repair commit the signed-off total remained 301; credit was added only
after the subsequent successful full run and independent review.
