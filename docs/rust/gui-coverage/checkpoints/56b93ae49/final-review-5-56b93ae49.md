# Final scoped review5 — 56b93ae49

Both assigned IDs are **eligible for bounded Linux FirstPass sign-off**. No selected-scope visual blocker found. This review approves neither pixel parity nor wider parents/aliases; canonical files remain untouched.

Source `56b93ae49337356aa4f81c79b12171397278247b`; [run37425380545](https://github.com/jkendall327/hydrus-mine/actions/runs/37425380545); artifact `11394399260`. Required `check` and `parity-models` jobs are green; native GUI696pass/0fail. Other-platforms was skipped, so this is explicitly Linux evidence. ZIP SHA256 `bcfa6c6a2a484c4c2d54c913e7c12bde502167ae2ce95c9917640f6cc8bb0ab1`. Detailed source/log/artifact hashes and claim assertions are in [the JSON report](/workspace/validation-reviews/final-review-5-56b93ae49.json).

Eligible IDs:

- `audit-options-popup-notifications-popup-window-toaster-make-a-short-lived-popup-on-cookie-header-updates-through-the-client-api`
- `import-existing-tags-filter`

Actually inspected all4 fresh native images and all3 declared Qt UI captures using `view_image`; recomputed image hashes match the fresh artifact manifest and exact-source checklist. Inspected source/reference files match git source56b93ae49. No missing exports or failed image reads.

| Inspected image | SHA256 |
|---|---|
| [api_update_toasts_cookie.png](/workspace/validation-reviews/ci-56b93ae49/native-renders/api_update_toasts_cookie.png) | `4b0937e5e380ed9f18523a4bb3a0752e8bf3c9f5e3250f7a06108c3f77a14b7f` |
| [api_update_toasts_header.png](/workspace/validation-reviews/ci-56b93ae49/native-renders/api_update_toasts_header.png) | `0f20d3e3187b02cfbf98a956a5a1ee3863b9ed90528a78140065a3abaf6544e5` |
| [api_update_toasts_options.png](/workspace/validation-reviews/ci-56b93ae49/native-renders/api_update_toasts_options.png) | `c13ddded3ea462310afb0f313f706f493d9e882485610135aac8f02133b07938` |
| [api_update_toasts_options_qt.png](/workspace/hydrus-mine/oracle/fixtures/api_update_toasts_options_qt.png) | `097f9b4daab2764dc4a93f8ca14d4a1c422e62533c2258f9bd233871691b3a61` |
| [api_update_toasts_popup_qt.png](/workspace/hydrus-mine/oracle/fixtures/api_update_toasts_popup_qt.png) | `f5ee0846d35219a56fdd2b5ae4d43faa054789ad0b5c160602c54782ca08c525` |
| [existing_tags_filter.png](/workspace/validation-reviews/ci-56b93ae49/native-renders/existing_tags_filter.png) | `4535a65072eef7df9e9a2972d2d369f64f12e847df78c2968c76bd0a51959bf7` |
| [existing_tags_filter.png](/workspace/hydrus-mine/oracle/fixtures/existing_tags_filter.png) | `bf9b01ad67275c8d44ad650a87c8d06dd6bb5a9a72f0bb034dd6c45689a7209b` |

API toast: the enabled checkbox has the exact Qt label and checked state, and native cookie text exactly matches the genuine Qt cookie PopupMessage. Header text `Headers sent from API: / Set: A-New / Set: Z-New` is readable in the fresh native frame and matches the actual Qt JSON plus authenticated API/native assertions. No visible toast/card clipping. The Options32/fixed width differs from Qt56/unfixed because the native capture follows the deliberate concurrent-width-merge test; it is not a selected-control mismatch. The cookie-only Qt PNG does not independently picture the header case. Native full Main/toaster wrapper and Qt private PopupMessage are different capture surfaces.

Scoped assertions and logged passes establish all recorded Apply/Cancel/reopen states, hidden/stale editor refusal, concurrent field preservation, authenticated20-case producer/no-op/error behavior, natural0/4/5/6-second Store/API expiry boundaries, real live persisted-deadline consumption, declined-close liveness, accepted-close retirement and rebind behavior. GUI retirement does not cancel independently persisted jobs or promise removing prior visible rows. No HTTP socket, placement, freeze-on-minimise/other-display, or wider toaster/network completion credit.

Existing-tags filter: both actual captures show whitelist selected, unnamespaced checked, namespaced unchecked, the unnamespaced rule and exact current-filter summary/explanation. Native three tabs, favourites controls, test input and Apply/Cancel are readable with no overlap. Qt has part of its testing area below its captured scroll viewport; native shows it fully. Existing disclosed differences remain: direct service-panel action rather than Qt cog action, detached owned Slint window rather than Qt modal dialog.

Scoped native/model/importer passes verify captured-service identity, child Cancel preservation, parent Apply blocking, accepted-only draft update/enabling, Store reopen, retained-child refusal after owner cancellation, untouched second service/parser fields and exact saved parsed/additional/disabled consumer results. Existing shared tag-filter language/favourites and wider per-service/import-option parents gain no new credit.

**API assessment cleanup recommended:** remove only `Conditional finite-control FirstPass awaits hosted execution/render review.` The actual exact-source required Linux jobs and scoped fresh render review have passed. Preserve the supported behavioral assessment and all scope limitations. Root must review the changed final packet digest and recompute final source/schema preflight before publication; this reviewer made no canonical edits.
