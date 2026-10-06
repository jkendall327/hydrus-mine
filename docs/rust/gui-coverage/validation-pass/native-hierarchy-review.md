# Curated native window hierarchy review

Baseline: `3702cdbee99473cfd7af073aa044fe847f78f405`. The source census has 121 exported windows; 24 lacked any navigable inventory association. The curated patch maps those 24 using 15 new canonical window nodes and nine existing canonical window IDs. In total it adds 66 nodes and updates 51 nodes, including grouped workflows, existing semantic descendants and actual menu/Options routes. It does not apply the lexical control proposal.

Every new or refreshed behavioral assessment is **Partial**. Existing consumer nodes updated only to add shared-editor references retain their prior assessments. Nothing in this patch creates a First Pass claim or a completed-leaf certification. CI execution, render comparison and final publication review remain pending centrally.

| Exported window | Canonical native ID | Change |
|---|---|---|
| ArchiveRepairWindow | `dialog.database.archive-repair` | add |
| ChoiceButtonsWindow | `dialog.shared.choice-buttons` | add |
| DatabaseLocationsWindow | `dialog.database.locations` | add |
| LocationMaxSizeWindow | `audit.media.database.locations.max-size` | update |
| GranularityWindow | `audit.media.database.locations.granularity` | update |
| DebugFetchWindow | `dialog.debug.fetch-url` | add |
| EditValueWindow | `dialog.shared.edit-value` | add |
| ExternalCallWindow | `audit.options.nested.external-call` | update |
| ExternalCommandWindow | `audit.options.nested.external-call.command` | update |
| ExternalDefaultsWindow | `dialog.external-call.defaults` | add |
| FileHistoryWindow | `dialog.database.file-history` | add |
| FileMaintenanceWindow | `dialog.database.file-maintenance` | add |
| GuiColourPickerWindow | `dialog.shared.rgb-colour` | add |
| HowBonedWindow | `dialog.database.how-boned` | add |
| LocalTransferWindow | `dialog.media.local-transfer` | add |
| MediaViewWindow | `dialog.options.media-view` | add |
| OpenFileCallsWindow | `audit.options.nested.open-file-call-list` | update |
| ExternalRoutingChoiceWindow | `dialog.external-call.routing-choice` | add |
| RelatedWeightsWindow | `audit.options.nested.tag-suggestions.weights` | update |
| ShortcutSetWindow | `audit.options.nested.shortcuts-set` | update |
| ShortcutCommandWindow | `audit.options.nested.shortcuts-command` | update |
| MostUsedTagsWindow | `dialog.options.most-used-tags` | add |
| TagSyncReviewWindow | `dialog.tags.sync-review` | add |
| VacuumReviewWindow | `audit.media.database.maintenance.vacuum` | update |

Choice buttons, bounded value editing and RGB colour picking are canonical shared helpers. Storage weights and rating editors point to bounded-value editing; granularity/rebalance link to choice buttons; colour Options owners link to the RGB helper. Most-used tags link to `catalog.writetagswindow`, matching `write_tag_window::open_additions` in the native caller. External callable input rules link to `dialog.url-class.default-processor`, matching `string_processor_window::open`. The debug fetch window follows the actual Help → debug → network → fetch a url route.

The patch preserves the material limitations: native colour picking is RGB-only; history/statistics/new-maintenance search uses typed predicates rather than reference autocomplete; tag sync is synchronous and lacks the reference background worker; vacuum exposes one native database with in-place VACUUM; shortcut command support is narrower than the full reference catalog. Granularity retains the checkpoint split-base publication/rollback defects, and new-maintenance callbacks retain the checkpoint retirement gap. Concurrent root repairs do not retroactively validate this pinned snapshot.

Patch source anchors include explicit baseline commits and line SHA-256 fingerprints; native constructor/callback sources, Python panel sources, authored tests and recorded fixture provenance are cited where available. Authored tests and fixture references are supporting provenance, with `executed_in_inventory: false`.

Read-only verification used the actual `gui_coverage.apply_patches` routine in memory, checked parent/shared targets and cycles, recalculated navigation metrics, and verified 513 patch anchor records. The merged source census reports **121 windows and zero opaque windows**. Existing inherited inventory anchors have not been repinned or fully validated by this review.

One pinned anchor differs from the concurrently modified working tree: `crates/hydrus-gui/src/vacuum_review_window.rs:107`. Its checkpoint fingerprint remains correct; final publication must repin and review the committed repair rather than silently substitute the current line.

Artifacts: `native-hierarchy-patch.json` is the schema-compatible patch; `native-hierarchy-mapping.json` is the 24-window mapping; `native-hierarchy-validation.json` records the limited read-only checks. No canonical inventories, HTML, source files or Git state were edited.
