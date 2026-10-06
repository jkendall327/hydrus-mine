# Linux validation checkpoint: 326 signed-off completions

Source `9bec37964725add7a4f6f072c6a9426ef2e3789b` passed [full Linux CI](https://github.com/jkendall327/hydrus-mine/actions/runs/37491826054). Seven independently reviewed original leaves bring the ledger from 319 to 326; 86 were newly signed off across five October 6 checkpoints, with 49 existing candidates awaiting sign-off. Windows and macOS remain deferred.

The seven additions cover three colour controls and four thumbnail preview-selection preferences. The RGB picker fits every channel and action at its opening and intrinsic minimum sizes. Actual checkbox/dropdown input establishes displayed colour state; thumbnail checkboxes respect disabled state, stage changes and preserve Store values until Apply. Existing consumer, cancellation, persistence and ownership assertions remain intact.

All 697 GUI tests passed with zero failures or ignored tests (142.79 seconds); all 67 media tests passed (1.01 seconds). Strict Clippy took 7m09s and test compilation 9m19s. These are hosted observations, not controlled compilation speedup measurements. Full workspace and parity validation passed. Fresh native captures and the desktop/narrow coverage report were inspected.

Evidence: [CI identity](ci-evidence.json), [outcome](validation-outcome.json), [full Linux log](linux-full.log), [parity log](parity-models.log), [Clippy log](clippy.log), [thumbnail review](final-review-2-9bec37964.md), [colour review](final-review-5-9bec37964.md), [root review](review.json), [selected changes](selected-patch.json), [image manifest](native-render-manifest.json), [browser checks](browser/browser-check.json), and [preservation audit](preservation-and-copy-audit.json).

All prior 319 approvals and unselected assessments remain unchanged. The RGB editor's source anchor was corrected to the named version of the same SpinBox; this earns no completion credit. Archived prior evidence stays immutable. Inventory statuses are separate: 927 First pass, 388 Partial and 497 Missing reference nodes, out of 1,812; 1,799 native entries and 121 exported windows.

Limits remain explicit: the picker is RGB-only, without Qt HSV/HTML/history features; QSS/OS palettes and unrepresented controls are outside scope. Controlled role-paint captures do not prove arbitrary physical-display timing or combined bitmap parity. Thumbnail preference review does not approve unrelated playback, duplicate controls or parent families. The next bounded batch targets idle-caption clipping, preview size evidence and the thumbnail clear-cache action.
