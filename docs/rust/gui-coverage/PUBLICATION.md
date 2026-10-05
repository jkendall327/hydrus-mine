# Preparing the next validated checkpoint

`scripts/gui_publish.py` stages coverage outside the repository. It never copies
files into canonical paths, changes a checkout, runs Cargo, pushes, or promotes
all author claims automatically. The root integration owner chooses and reviews
the exact source checkpoint and publication scope.

Historical author commits named by parity packets may differ from the integrated
commit identities because integration uses cherry-picks. Immutable annotated
`gui-evidence/eighth-b010/*` tags preserve the original source objects needed by
historical hash audits. Fetch these in a fresh clone before inspecting those pins:

```sh
git fetch origin 'refs/tags/gui-evidence/eighth-b010/*:refs/tags/gui-evidence/eighth-b010/*'
```

These tags preserve provenance only. They do not attest to runtime validation or
promote coverage; use the exact integrated source and its hosted CI for that.

Run preparation with the full source SHA and a fresh dedicated output directory:

```sh
python scripts/gui_publish.py prepare --source FULL_SOURCE_SHA --out /tmp/gui-review-CHECKPOINT
```

Preparation reads committed source, inventories, the prior ledger and author
packets at that SHA. It verifies the frozen `4356918f41d2cb2b0159eb8c466d987de9fa9339`
baseline: 1,812 original IDs, 862 Missing, 378 Partial and 572 First pass.
It does not repin inventories or claim tests passed. It writes:

- `source-census.json`: every exported Slint Window, associated inventory IDs,
  callback/property/widget/control anchors and shared component definitions.
- `claim-proposals.json`: author proposals with frozen-leaf eligibility and
  prior-validation flags; eligibility is not approval or runtime proof.
- `native-hierarchy-proposal.json`: source-only Partial proposals for windows
  without assessed child/shared-editor navigation. These need semantic grouping,
  reference associations and regression review before publication.
- `review-template.json` and `preparation.json`: source/census fingerprints,
  empty approval lists and scope limits.

At checkpoint `792d874a5aa3a86d01cb7379d80ed4f4aab75dd9`, the source census has
92 exported windows. The existing canonical hierarchy covers 89; new proposals
identify `CommandPaletteWindow`, `LoginDomainEntryWindow` and
`TagMigrationProgressWindow`. All windows are inspected individually, without an
opaque “other windows” bucket. Census records include read autocomplete tab
callbacks in `MainWindow`, `ManageTagsWindow` and shared components; the
predicate custom-default callbacks and shared `DefaultsButton` star control are
also visible. A lexical record is a source-review aid, not a semantic feature
count: properties are not automatically turned into user-facing leaves, repeated
widgets/actions may need grouping, and runtime-generated rows need review of
their Rust model and consumers.

For a component with several modes, keep one physical-window entry and group
the mutually exclusive controls beneath it. Follow shared custom components
into their definitions and link their established editor assessments. Recursive
owned children reuse that window component: record recursive ownership as a
relationship, never a self `parent_id` or a second physical-window count. Review
the explicit owner slots, child blocking, accepted callbacks, cancellation and
retired handles alongside the Slint controls. A source census cannot establish
those behaviors. New hierarchy groups remain Partial until their exact scope is
reviewed; an authored native-component descriptor is a proposal, not approval.

Rebuild the candidate after repairs land. The final source SHA, all supplied
anchor hashes, edited-ID lists and patch/census fingerprints must describe that
same checkpoint. Green individual CI jobs at an earlier SHA remain historical
evidence until the final source's complete run succeeds.

The reviewer creates a curated patch using the existing `gui_coverage.py` patch
schema. Its `baseline_git_head` must be the full source SHA. Reference additions
and all inventory deletions are forbidden. Every source/evidence anchor supplied
by the patch must explicitly set `anchor_git_head` to that source, use a valid
relative path/line and carry its exact `line_sha256`. This makes stale author
line numbers fail instead of silently resolving to nearby implementation code.
For partial field updates, omitted fields keep inherited source provenance;
changed inherited lines require a reviewed replacement anchor.

Fill the review packet with:

- `source_commit` and a nonempty `reviewed_by`.
- `patch_sha256`: `gui_publish.digest(patch)`, a SHA-256 of parsed JSON with
  sorted keys, independent of indentation. The same helper computes fingerprints
  for all review inputs.
- `reviewed_assessment_ids`: exactly all reference patch update IDs.
- `reviewed_native_ids`: exactly all native patch update/addition IDs, including
  any new First pass controls; new native controls never increase original-ID
  completion totals.
- `selected_completion_ids`: only the additional original concrete leaves that
  the reviewer accepts as implemented. They must have an author manifest at the
  source checkpoint, a frozen Missing/Partial before value and reviewed final
  First pass scope. Parents, aliases, evidence-only work, explicitly excluded
  leaves and previously completed IDs cannot be selected.
- `reviewed_source_census_sha256`: the preparation fingerprint after reviewing
  all Window coverage, control granularity, tabs and custom/shared controls.

Reviewing the fingerprint does not approve the raw hierarchy proposals; the
curated patch and explicit native IDs determine the published changes. Newly
promoted countable original leaves must also be explicitly selected. Unselected
author proposals remain pending; their existence at the tested source does not
promote their coverage status.

After the root owner obtains exact successful hosted CI, stage publication:

```sh
python scripts/gui_publish.py publish --source FULL_SOURCE_SHA \
  --patch /tmp/gui-reviewed-patch.json --review /tmp/gui-reviewed-scope.json \
  --ci-evidence /tmp/gui-exact-ci.json --out /tmp/gui-publication-CHECKPOINT
```

CI evidence must name matching full `source_commit` and `head_sha` (or
`headSha`), a completed successful run in this repository, and exactly the
completed successful `check`, Windows, macOS and `parity-models` jobs from that
run. Required formatting/Clippy/tests/ratchet and build/backend steps must be
present and successful. This validates the supplied evidence; the root owner
must collect it from the actual GitHub run. The script does not authenticate
JSON or query GitHub, and a green run does not independently assess every
control or prove reference recording execution.

Staging validates inherited and authored fingerprints, uniquely remaps unchanged
source lines/context, rejects changed/ambiguous anchors, recomputes aggregates
and candidate mappings, rejects weak First pass claims, and checks all source
windows for assessed nested/shared-editor navigation. Window counts come from
source, without an 89-window default. Historical boundaries remain in per-node
evidence/assessments; inherited validation objects are not rewritten as newly
assessed behavior. Only edited validation objects are refreshed.

The cumulative ledger unions explicitly selected original leaves with prior
validated completions, preserves the frozen before counts and excludes native
IDs, parents, aliases and evidence-only assessments. The cumulative patch must
replay every resulting node from the frozen inventories. The tool renders HTML
using the source checkpoint's own coverage helper and standalone embedded-data
template. No external runtime service is added.

All checks finish before a fresh output directory is created. Outputs include
the two inventories, cumulative progress/replay patch, exact CI evidence,
source-anchor/fingerprint audit, HTML and a publication summary. The root owner
reviews and copies desired staged files, updates the narrative notes/ROADMAP
and README counts, performs the browser check, commits and pushes. Staging does
not claim an interactive browser check. Canonical notes are deliberately not
generated from source-only proposals.

Fast regressions use `python scripts/test_gui_publish.py`; they cover wrong
source/run/step evidence, frozen-leaf counting, source census and shared star/tab
anchors, failed publication without output, and source-only preparation without
promotion. A complete nonempty-selection staging regression rolls one historical
leaf out of an in-memory prior ledger, restores it through publication, verifies
frozen IDs/cumulative replay/HTML, and rejects unselected, duplicate,
noncountable and wrong-source inputs. Its CI is explicitly synthetic and outputs
exist only in a temporary directory; it supplies no hosted validation evidence.
No Rust build or mutation testing is required for this helper.
