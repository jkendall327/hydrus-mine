Independent idle review: `c768fef48120cdf529656285b8b77aac1a5b8e27`

Approve exactly the two selected browsing/API leaves for Linux. Run `37504670811` and fresh artifact `11433007448` passed required check/parity jobs, strict all-target Clippy, full workspace tests and ratchet. GUI: 698 passed, 0 failed, 0 ignored (229.18s); media: 67 passed, 0 failed, 0 ignored. Windows/macOS are deferred. No canonical edits, local builds or publication were performed.

Selected IDs:

- `audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-idle-permit-idle-mode-if-no-general-browsing-activity-has-occurred-in-the-past`: approval **true**.
- `audit-options-maintenance-and-processing-when-to-run-high-cpu-jobs-idle-permit-idle-mode-if-no-client-api-requests-in-the-past`: approval **true**.

I separately viewed all five fresh captures listed below and matched each file hash to the exact-source artifact manifest. At both sizes, finite browsing/API controls show enabled 1-minute spinners and unchecked full ignore captions; ignored controls show checked full captions with disabled 1-minute spinners. Captions, units and selected widgets are fully visible with no observed overlap. The CPU ignore caption remains clipped at the right edge and is outside this approval.

| Fresh capture | SHA256 |
|---|---|
| [idle-timeout-finite-900x640.png](/workspace/validation-reviews/ci-c768fef48/native-renders/idle-timeout-finite-900x640.png) | `5a6d21bd5909161c67dd96939881ffe8aa0df6fb7e9c37bc2a7e7882ec5d8386` |
| [idle-timeout-finite-1000x850.png](/workspace/validation-reviews/ci-c768fef48/native-renders/idle-timeout-finite-1000x850.png) | `852c90073a45ff8dcf8f9cf2ee9fef062ea4715658f4e3778bddd2190b4e09cc` |
| [idle-timeout-ignored-900x640.png](/workspace/validation-reviews/ci-c768fef48/native-renders/idle-timeout-ignored-900x640.png) | `4c316a027694a5fabb1071ceb55007ec48d97960c194f89a1c697dc2a5ad38a8` |
| [idle-timeout-ignored-1000x850.png](/workspace/validation-reviews/ci-c768fef48/native-renders/idle-timeout-ignored-1000x850.png) | `4bf6b6b0a30c234380af88d9b16ac8548fb3a357edb9def9decce1118402b388` |
| [idle-timeout-options.png](/workspace/validation-reviews/ci-c768fef48/native-renders/idle-timeout-options.png) | `4bf6b6b0a30c234380af88d9b16ac8548fb3a357edb9def9decce1118402b388` |

The legacy options capture was separately viewed but is byte-identical to the ignored 1000×850 capture; it supplies no extra visual state. ZIP SHA256: `6de6fd545b79f9bec83f8b742ca304d40e993bfb8e00198d5bc87ce981776843`.

Behavior is supported by exact-source native tests for staged edits, Cancel, unchanged raw-seconds acceptance, saved live eligibility, strict boot/activity boundaries, actual idle-only autosave backup, persistence/reopen, and constructor-bound actual widget geometry/state. Model tests replay actual Qt floor/clamp/None retention and gate recordings, field merges and concurrent normalization. Import retains raw seconds/None/defaults. A separate backend test exercises actual rejected/read API requests while SQLite is locked. All ten exact passing log lines and twenty verified source/reference/test file hashes are recorded in the JSON.

Source anchors: Options idle-only layout `crates/hydrus-gui/ui/options.slint:447`; native staged consumer test `crates/hydrus-gui/tests/gui/idle_timeout_options.rs:50`, raw acceptance test `:177`, geometry/state test `:242` and four capture save site `:342`; model replay `crates/hydrus-gui-model/tests/model/idle_timeout_options.rs:79`; Qt constructors `hydrus/client/gui/panels/options/MaintenanceAndProcessingPanel.py:32`; actual Qt recordings `oracle/fixtures/idle_timeout_options.json` and `oracle/fixtures/idle_timeout_constructors.json`. Exact line/file hashes and bounded consumer anchors are in the JSON.

Limitations retained:

- Linux-only approval at exact c768 source/run/artifact. Windows and macOS remain deferred.
- Only browsing/API editable thresholds and the existing live idle/session-autosave eligibility consumer qualify. No high-CPU maintenance jobs, CPU-busy detection, idle enable/force/debug/shutdown control, parent or alias completion credit.
- The pictured mouse control is unselected layout support and remains Partial: application-window movement is observed, but global OS cursor movement is unsupported. No mouse leaf completion credit.
- Neither idle Qt recorder exports a matching idle-control PNG. Behavior is grounded in actual Qt source/JSON recordings; fresh native appearance is independently inspected, without same-state screenshot or exact-pixel parity. The force-idle Qt PNG belongs to another surface.
- Qt uses horizontal number/unit/None layout; native selected idle rows use two lines for complete captions. The other 22 generic Noneable layouts retain their previous branch.
- The new geometry test observes actual constructor-bound checkbox/spin state at both sizes; it does not dispatch physical checkbox clicks. Existing staged callbacks, Apply/Cancel, persistence/reopen and live consumer assertions supply the behavioral evidence.
- Strict intrinsic checkbox width and whole-window bounds remain asserted without tolerances; they do not independently measure inner ScrollView clipping/footer or adjacent-row overlap. Actual fresh images confirm selected captions/units/controls are visible at both tested sizes.
- The unselected CPU ignore caption is visibly clipped at the right edge. This review gives it and the maintenance parent no completion credit.
- GUI API consumer regression writes the timestamp marker used by middleware; separate backend actual-request tests cover rejected/read requests while SQLite is locked. This is not a combined physical GUI plus HTTP replay.
- No new idle-specific hidden/closed/rebound retained Apply replay is added. Generic Options active/visible owner guard is source-inspected; no new owner-retirement regression is claimed.

The prior bd11 compiler failure remains historical evidence. The c768 correction only moves an owned clone into the writer closure and retains the original expected settings and both Cancel comparisons; current full CI resolves that blocker. Previous 326 completions are unchanged, and this review grants exactly two selected leaves, with no parent or neighboring control credit.

Evidence: [`ci-evidence.json`](/workspace/validation-reviews/ci-c768fef48/ci-evidence.json), [`linux-full.log`](/workspace/validation-reviews/ci-c768fef48/linux-full.log), [`parity-models.log`](/workspace/validation-reviews/ci-c768fef48/parity-models.log), and [`native-render-manifest.json`](/workspace/validation-reviews/ci-c768fef48/native-renders/native-render-manifest.json).
