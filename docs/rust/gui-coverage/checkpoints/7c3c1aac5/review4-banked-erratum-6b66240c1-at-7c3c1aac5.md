Two statements in the archived 6b review incorrectly described separate leaves as already banked. This erratum corrects those statements only. The six reviewed approvals, all behavioral/render caveats, original archived bytes and the 301-completion total remain unchanged. Neither separate leaf receives credit.

Checked against the exact7c3 source ledger (`docs/rust/gui-coverage/overnight/progress.json`): 301 distinct completed IDs, published source 6b. Both separate original IDs are absent from that ledger and remain Missing in the reference inventory. The native show/hide-sidebar menu node is Partial.

Incorrect literal:

> Global hide-preview control stays Partial/count=false, including first-raster acceptance/restoration limitations; no parent or alias credit. Already banked show/hide-sidebar leaf is supporting behavior only.

Corrected literal:

> Global hide-preview control stays Partial/count=false, including first-raster acceptance/restoration limitations; no parent or alias credit. The separate show/hide-sidebar-and-preview original leaf is not in the 301-completion ledger and remains Missing in the reference inventory; its native menu node is Partial. Its behavior is supporting evidence only, with no completion credit.

Affected previously selected IDs:

- `audit-options-menu-menu-pages-restore-all-pages-sidebar-preview-sizes-to-saved-value`
- `audit-options-menu-menu-pages-save-current-page-s-sidebar-preview-size-now`
- `audit-options-menu-menu-pages-save-current-page-s-sidebar-preview-size-on-client-exit`
- `audit-options-sidebar-splitters`

Incorrect literal:

> Existing tab-alignment leaf is already banked: four-side captures/source are support only, with no additional credit. Broader GUI Pages/navigation/tab parents and unrelated drag/context actions remain Partial.

Corrected literal:

> The separate notebook-tab-alignment original leaf is not in the 301-completion ledger and remains Missing in the reference inventory. Four-side captures/source are supporting evidence only, with no alignment completion credit. Broader GUI Pages/navigation/tab parents and unrelated drag/context actions remain Partial.

Affected previously selected IDs:

- `audit-options-gui-pages-navigation-and-drag-and-drop-experimental-hide-main-page-navigation-tabs`
- `audit-options-gui-pages-page-tab-names-when-there-are-too-many-tabs-to-fit-elide-their-names-so-they-fit`

The separate assertion that tree navigation was previously banked is correct: `audit-options-gui-pages-navigation-and-drag-and-drop-experimental-show-tab-tree-view` is in the frozen completed ledger. No other incorrect specific banked assertion was found in this historical review. The historical prior count 280 describes the pre-publication baseline and is unchanged.

Optional unapplied reference-only proposal: `review4-banked-erratum-reference-patch-7c3c1aac5.json`. It copies the six exact7c3 canonical nodes, changes only the two literal limitations above and repins source anchors using the frozen checked `gui_coverage.repin` implementation. Its proof verifies all statuses, assessments, implementation validation and all other non-anchor content are preserved. Native updates and new completion selections are empty. Link this narrative erratum separately; do not replace or rewrite the archived review.

Archived JSON SHA-256: `d45072a75e28d1df37bcb812e8e932b3aef62e506363ca8566e7c98c6934fb2f`. Archived Markdown SHA-256: `17074b95ed736f1895f66bc6e52f4c77e5959628fab0981b126750eb59ded423`. Ledger SHA-256: `203522917211a2a09df82be1d82f4b33320a2e359a29f02d4d06aa503cd80bdb`.
