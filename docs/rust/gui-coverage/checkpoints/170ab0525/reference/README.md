# Predicate Qt parent destruction: bounded reference recording

Reference source: `1eace8c6ef5a1ed6a2367ad48bdbed28800f6484`. Scratch only; no repository edits, Rust builds, status changes, or native approval.

The isolated run exited 0. Its `parent_destruction` records two successful cases, filesize and hash. Each used an owned standalone QWidget, `launcher.window()` as the actual production `DialogEdit` parent, `hide_buttons=True`, and an actual `FleshOutPredicatePanel` containing the production editor. Dialog/panel/editor were shown before deletion. The standalone owner is deliberately independent of the client's Main.

The recorder retained every Python wrapper, connected real QObject `destroyed` signals, called only `owner.deleteLater()`, and then drained eight rounds of `sendPostedEvents(None, QEvent.DeferredDelete)` plus `processEvents(AllEvents, 10)`. It did not hide, close, reject or release a child first. Immediately after posting deletion all five wrappers were valid; after the bounded drain owner/launcher/dialog/panel/editor had all emitted destruction and were invalid. Both original predicate values remained `None`; the client Main remained valid. There were no captured Qt callback exceptions. This proves these shown nonmodal child cascades, not destruction while a DialogEdit `exec()` loop is active or native Main/search ownership.

## Proposed existing-path extension

`hash-predicate-lifecycle.proposed.patch` changes only existing `oracle/record_hash_predicate.py` and `oracle/fixtures/hash_predicate.json`. It adds the two successful nonmodal lifecycle records after the old recorder restores its warning/question mocks. No lifetime handler is mocked. All ten original top-level fixture keys remain exactly value-identical, including every query, cleanup, key, acceptance, warning transport and Cancel case. No new repository input paths are proposed. The hash recorder also records the filesize cascade so a second recorder/fixture extension is unnecessary.

`record_hash_predicate.proposed.py` and `hash_predicate.proposed.json` are ready for root review/adaptation. The proposed helper omits the failed nested path and diagnostic checkpoints. Root should run the adapted committed recorder; the preserved executed script is `record_hash_predicate.executed-success.py`, not the later simplified proposal. `hash_predicate.extended.json` is the raw successful run output; it retains the separate historical failed-attempt note. The proposed fixture drops that note and narrows its scope description, keeping the actual two successful observations unchanged.

Actual successful execution command (the scratch proposal at that time is preserved byte-for-byte as the executed-success script):

```bash
source /workspace/.cloud/hydrus/activate.sh
timeout 150 /workspace/.cloud/hydrus/pyenv/bin/python /workspace/validation-reviews/predicate3/reference/record_hash_predicate.proposed.py --child /workspace/validation-reviews/predicate3/reference/hash_predicate.extended.json > /workspace/validation-reviews/predicate3/reference/record_hash_predicate.log 2>&1
```

For reproducing those exact script bytes now, substitute `record_hash_predicate.executed-success.py` for the script filename. The preserved script points imports to the existing repository oracle and writes captures to this scratch directory's `fixtures`. The proposed canonical script retains the original portable HERE/import/main behavior. The environment provides Python 3.13, existing Qt libraries, and `QT_QPA_PLATFORM=offscreen`; no installations or environment changes were made. The client runs on a copied basic fixture with network traffic paused. Existing temporary localhost Client API binding failed under sandbox permissions in both runs; GUI hook execution succeeded in the isolated run. No API/service functionality is claimed.

## Separate failed nested warning attempt

`record_hash_predicate.failed-attempt.py` and its matching `.log` preserve the first execution. A QtTest pointer click invoked the real hash cleanup callback with the existing bad-lines-normal input, and the actual unchanged `ShowWarning` / static `QMessageBox.warning` produced `fixtures/hash-predicate-parent-destruction-warning-qt.png` before the deletion attempt. The process then aborted with exit 134 and `double free or corruption (out)`. It produced no complete result fixture. The cause is unestablished; no successful nested-destruction observation or parity claim follows from that abort. Do not rerun or port that failure as intended behavior. Subsequent work explicitly skipped it; no further nested deletion experiments occurred.

The real warning transport and acknowledgement in the original recorder remain separate successful evidence. That capture, `fixtures/hash-predicate-warning-qt.png`, and the failed-attempt nested warning capture show the same four cleanup error sections (unknown prefix, nonhex, odd length, unusual length). Both were actually viewed, as was `fixtures/hash-predicate-qt.png`. Widget grabs omit window decorations; warning titles/modal state are recorded in the original fixture's `warning_transport`.

## Source-derived ownership and remaining limits

`ClientGUISearch.py:146–163` derives `widget.window()` and constructs the production DialogEdit/FleshOutPredicatePanel. Existing edit does likewise at `:69–96`. `ClientGUITopLevelWindows.py:557–561` and `QtPorting.py:613–626` forward the parent to QDialog. `ClientGUIPredicatesSingle.py:1230–1250` invokes the real cleanup warning; `ClientGUIDialogsMessage.py:14–33,56–58` passes its widget to static QMessageBox.warning on the Qt thread. Those source facts establish parent routing, not successful deletion of an active nested static warning. The safe native notice teardown regression can remain a separately scoped safety boundary, backed by successful editor ownership and warning transport evidence, with the nested Qt limitation retained.

No generic active-edit/global-radio, parent/alias, persistent query publication, OS decoration, bitmap parity, Windows or macOS credit is added. Full native exact-source Linux CI and independent fresh rendering remain separate gates. `provenance.json` binds scripts, raw/derived results, source/input files, logs and actually viewed images.
