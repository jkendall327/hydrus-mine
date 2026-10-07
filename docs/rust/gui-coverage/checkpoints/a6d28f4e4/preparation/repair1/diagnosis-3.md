# MIME choice setup repair

The failed source was `8ada16c768f07d89c676b0ec09a6a56150f26ace`, Linux run 37609745626. Its new outer-MIME test failed at `select_mapping` because the intended static-GIF row was absent. The preserved raw log is `/workspace/validation-reviews/ci-8ada16c76/full-check.log` (failure around line 2672).

The committed Qt fixture contains two `image/gif` chooser labels: index 9 is animated GIF (code 3), and index 54 is static GIF (code 68). The test helper admitted the first matching label, then looked for the static-GIF human caption. Production admission uses the ordered typed MIME vector. This is a deterministic test setup ambiguity, not demonstrated evidence of a production missing-row, font, rendering or runner-image defect.

Repair commit `13cff28c2db3be013d7b7bcaf5b213a410c5d233` selects the exact recorded MIME code from the complete eligible recorded sequence. It asserts the full actual chooser labels before dispatch, then the intended human-caption row after child Apply. Filtering used MIME captions is source-derived; it is not claimed as a separately recorded post-PNG chooser. Missing-row diagnostics now report enum, code, human caption, MIME label, visibility and full visible draft. The static-GIF successor scenario and every existing behavioral assertion remain.

Only `crates/hydrus-gui/tests/gui/open_externally.rs` changed (49 inserted lines, two replaced non-assertion lines). All 358 failed-source assertion macros remain, with two added assertions; seven tests remain. The banked 249-assertion file is still the exact bytewise prefix. Production is unchanged. Frozen prior proof packets are in `failed-source-8ada16c76/`; new proof is `assertion-preservation-3.json` and the exact repair is `repair.diff`.

Rustfmt and diff checks passed. Independent review 2 found no concrete source/API/sequencing blocker. No local Cargo build, runtime execution or push was performed. The separate menu-choice-wheel failure remains root-owned. New exact-source full Linux CI and fresh image inspection are required before any approval or publication.
