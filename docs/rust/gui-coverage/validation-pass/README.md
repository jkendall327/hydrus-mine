# GUI validation and delivery pass

The pass starts from merged source `3702cdbee99473cfd7af073aa044fe847f78f405`.
The previously published checkpoint remains `028fd72f` (240 original leaves).
Source review of all 153 parity manifests is recorded in `review-1.md` through
`review-6.md`; these reports describe the initial source, not later repairs.
The six `demotions-*.json` files record explicit credit decisions. They exclude
previously validated IDs. Structural parents may retain a scoped assessment but
receive no completed-leaf credit.
The supplemental `demotions-render-evidence.json` defers seven further claims
whose defining native behavior lacks assertions, leaving 135 candidates.

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
