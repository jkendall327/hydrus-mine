# All individual feature leaves

The goal is every individual feature leaf implemented and explicitly verified.
The current report contains 1,274 distinct original terminal leaves after the
source-proven shared-alias crosswalk: 813 originally partial/missing and 461
historical first-pass leaves. The approximate 800 figure describes the original
implementation deficit, not the whole verification checklist.

At the `e00ddab707` validation checkpoint, 385 have explicit sign-off: 376
implementation completions and nine separately reviewed historical preset
verifications. The remaining 889 leaves have no explicit sign-off. No proposed
candidate from this batch remains awaiting sign-off. Historical first-pass status
earns no automatic fresh verification credit. Structural parents, aliases and
native surfaces do not add
individual completions. The source-pinned audit retains the one alias adjustment
from the original 1,275-terminal topology and the complete 1,812-ID crosswalk.

`checklist.json` is derived bookkeeping, not an approval mechanism. Implementation
flags retain the published deficit-leaf ledger's definition. The separate
`verification-ledger.json` retains append-only, independently reviewed batches
for original historical first-pass leaves. Its initial empty ledger grants no
credit. The report and checklist count the deduplicated union, with implementation
and historical verification totals displayed separately.

`python3 scripts/gui_verify.py --source FULL_SHA --manifest MANIFEST --review REVIEW
--ci-evidence CI --artifacts PACKET_ROOT --out EXTERNAL_DIRECTORY` stages a batch
outside the repository. Each original leaf needs explicit behavior/limitations,
reference replay, native behavior assertions and directly inspected fresh PNGs.
Source files and runtime artifacts are hashed; review binds the manifest, selected
IDs and exact successful Linux CI. The packet root contains artifacts at their
intended repository-relative checkpoint paths. Preparation and inventory labels
never grant credit. Root still reviews/copies the staged report and evidence,
browser-checks it, commits, and merges only after validation.

Every prior batch retains its original source, CI, review and artifact hashes.
Later inventory repinning does not imply retesting. Both report checks and later
implementation publication validate inherited evidence and reject changed/dropped
approvals, ineligible/duplicate IDs and implementation-credit inflation. Windows
and macOS remain deferred under the pinned publication policy.

The audit at source `05000f10e` records the preceding 334 ledger; the current
checklist incorporates the five independently approved presentation/appearance
leaves, two suggested-tag width/layout leaves, two namespace Add/Delete leaves,
the three filesize/hash leaves, the radio Return preference, four sort/group
controls, three viewer-eye preferences, the sidebar tag-display action and two
command-editor controls, registered-call Delete/Duplicate and Add Defaults, plus
URL Add/Edit, nested File Choose/Add/Edit/Order and outer MIME mapping
Add/Edit/Delete, live rating configuration examples and selected thumbnail
clear-deletion records and three thumbnail-navigation preferences without
changing the exhaustive goal IDs or exclusions.
