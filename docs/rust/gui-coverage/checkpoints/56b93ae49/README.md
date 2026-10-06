# Linux checkpoint: 20 validated completions

This checkpoint adds 20 independently reviewed original feature leaves to the
prior 240, for **260 signed-off completions**. Full Linux validation passed at
`56b93ae49337356aa4f81c79b12171397278247b` in
[run 37425380545](https://github.com/jkendall327/hydrus-mine/actions/runs/37425380545).
All 696 native GUI tests passed, with zero failures or ignored GUI tests.
Strict all-target Clippy, full workspace tests, reference/backend replays and
the parity ratchet passed. Windows and macOS were explicitly deferred under
the source-pinned owner policy; historical cross-platform evidence is preserved.

[Root review](review.json) binds the selected IDs and complete changed patch to
the source census and final patch digest. [CI evidence](ci-evidence.json),
[workspace log](linux-full.log), [reference/backend log](parity-models.log) and
[validation outcome](validation-outcome.json) preserve the exact run evidence.
The [selected patch](selected-patch.json) promotes only the selected 20; other
reference edits repair source anchors. Native inventory changes add no original
feature-completion credit.

Independent reviews:

- [File-view removal and importing work slots](final-review-1-56b93ae49.md) ([hashes and decisions](final-review-1-56b93ae49.json)).
- [Sibling colours](final-review-2-56b93ae49.md) ([hashes and decisions](final-review-2-56b93ae49.json)).
- [Autocomplete Children and Favourites](final-review-3-56b93ae49.md) ([hashes and decisions](final-review-3-56b93ae49.json)).
- [Formatting controls and popup actions](final-review-4-56b93ae49.md) ([hashes and decisions](final-review-4-56b93ae49.json)).
- [API update toasts and existing-tag filtering](final-review-5-56b93ae49.md) ([hashes and decisions](final-review-5-56b93ae49.json)).

These are bounded behavior approvals, not claims of complete parent-feature or
pixel parity. Reviews distinguish actual Qt images from recorded text/JSON,
and preserve missing-image, toolkit, wrapping, ownership and scheduling caveats.
Absolute scratch paths in original review records identify review provenance;
the selected native captures below are retained here independently of GitHub
artifact expiry. Reference images remain committed under `oracle/fixtures`.

## Retained selected captures

- [api_update_toasts_cookie.png](native/api_update_toasts_cookie.png) — `4b0937e5e380ed9f18523a4bb3a0752e8bf3c9f5e3250f7a06108c3f77a14b7f`.
- [api_update_toasts_header.png](native/api_update_toasts_header.png) — `0f20d3e3187b02cfbf98a956a5a1ee3863b9ed90528a78140065a3abaf6544e5`.
- [api_update_toasts_options.png](native/api_update_toasts_options.png) — `c13ddded3ea462310afb0f313f706f493d9e882485610135aac8f02133b07938`.
- [autocomplete_tab_children.png](native/autocomplete_tab_children.png) — `45996623217bdce62350b1da133db2a3d1a8d73c5f0b230f3335152ee0896d0f`.
- [autocomplete_tab_select_all.png](native/autocomplete_tab_select_all.png) — `42e11e2f767a98faa856cc218c04af2f5398e572a0e0ed965fc30c77b9ee0514`.
- [existing_tags_filter.png](native/existing_tags_filter.png) — `4535a65072eef7df9e9a2972d2d369f64f12e847df78c2968c76bd0a51959bf7`.
- [files-view-removal.png](native/files-view-removal.png) — `46a4ef78336105b9947b591459f9497f0b58e9b8bc4f69cd74fd7cd02b7807b0`.
- [gui_format_options.png](native/gui_format_options.png) — `569608a0db93974d466a51884fd858ca5a2b94f72698bcae3e37fa9ccfc284e9`.
- [options_import_work_slots_loaded.png](native/options_import_work_slots_loaded.png) — `297dfac86240bb2f58ef533e134ca3d2b8c29dc51331e48333b43851830a3897`.
- [popup_job_actions.png](native/popup_job_actions.png) — `1b92dd7373c7d8be727814eab7795663cb71cd38345c79a6dbd5a0b8ba85b671`.
- [popup_job_question.png](native/popup_job_question.png) — `fbb7ee3db4ceb570656df3eb23394eb8a667639d54697c9edc5dfffaf6d5bfe3`.
- [popup_job_question_dismissed.png](native/popup_job_question_dismissed.png) — `9d744f26103ce2cc4f43761020acb4d33c2fef09f5a16db7e1518a0aa2cbb1c0`.
- [popup_job_question_fixed32.png](native/popup_job_question_fixed32.png) — `33f4f151636c9101061d0d251591a73be2b4afbe8764e9594a7316219f95bc43`.
- [popup_job_question_narrow16.png](native/popup_job_question_narrow16.png) — `5e60bb6e92041b43c97121a71c07ce8c81a694f2ad4d8c723331e30ec3836291`.
- [sibling-colours-0-selected.png](native/sibling-colours-0-selected.png) — `a4d6bfac5b4fcec5d6eb2a96fcce40914a1cf666569d807a67e0593bc9e05456`.
- [sibling-colours-0-unselected.png](native/sibling-colours-0-unselected.png) — `cc8325dbfa432bf5798a1e98c66359e422a08d95fb21f3d2f8af9b85651aca9d`.
- [sibling-colours-2-selected.png](native/sibling-colours-2-selected.png) — `645648c3bd6b0cd64d47640521ae38a07a034f8fd364f5a7206f8d51406cd331`.
- [sibling-colours-2-unselected.png](native/sibling-colours-2-unselected.png) — `bfd82d371802730f45bfc6cba9ab317a46ce1e25efc869745fb8319745c11cf2`.
- [sibling-colours-collapsed-1100.png](native/sibling-colours-collapsed-1100.png) — `fe011509b6205d3874c878cf0fe9b582d8b5ba7025015c78d6f2c4e8544a545a`.
- [sibling-colours-collapsed-760.png](native/sibling-colours-collapsed-760.png) — `3d1c06b6fe1517f52f2cd044d060593383d76d4d9e7f5b862e926aa25225bb2a`.

The complete [artifact manifest](native-render-manifest.json) records all 266
images and the downloaded ZIP hash; only the 20 captures supporting this
checkpoint are duplicated here.

## Ordinary validation build observations

The UI-changing preceding run took 6m18s for Clippy and 6m34s to compile the
test profile. This test-only repair run reused the generated UI and took
1m29s and 4m33s respectively. GUI runtime increased from 129.58s to 216.45s;
these shared-runner observations are not a controlled cold-build benchmark.
No peak-memory or isolated linking improvement is established. The bounded
compilation investigation remains closed.

## Report verification

The final generated report passed actual Chromium checks at desktop and390px
width:260 cumulative IDs,20 new IDs, Linux/deferred-platform labeling, both
inventory views, filters, searches and pinned source links. There were no
JavaScript/request errors or horizontal overflow. [Browser proof](browser/browser-check.json),
[desktop capture](browser/report-desktop.png) and [narrow capture](browser/report-narrow.png)
retain the checked HTML hash and final rendered output.

After this checkpoint,115 candidates remain:42 prepared for final revalidation
and73 unresolved. They receive no completion credit from this publication.
