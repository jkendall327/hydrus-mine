# Deferred GUI validation pass

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
