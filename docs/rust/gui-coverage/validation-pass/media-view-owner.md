# Integration status

This report records a read-only review/proposal before integration. Root applied
the proposal afterward; native compilation and execution remain pending. In the
repair review, both the shutdown-test setup and separate vacuum admission findings
have been addressed. Backend selected-file tests and strict repair Clippy pass.
The reports below do not establish final-source hosted or rendered validation.

# Media-view child ownership proposal

Unapplied proposal: `media-view-owner.patch`. No source/build/test/commit actions performed. Proposed implementation was formatted with the supplied Rust 1.94 formatter; `git apply --check` passes. Hosted native tests and central CI/render review remain pending.

Reference: `hydrus/client/gui/panels/options/MediaPlaybackPanel.py:543-568` returns on a cancelled Add chooser, opens the child editor modally, and inserts only an Accepted result. `:572-593` likewise replaces an edited row only on Accepted. The parent Options draft owns those list edits until Options Apply.

Fix: one distinct shared lifetime flag for each chooser/editor chain, invalidated on replacement, child Cancel/native close, successful child Apply, and parent cancellation. A retained cancelled/closed editor cannot call draft acceptance again; a retired Add chooser cannot open a new editor, and both parent and chain must still be alive. Children refresh the parent child-open property when they open/close. Options Apply now checks actual media-child visibility, and its button respects the corresponding property, preserving the reference modal boundary.

Regression scope: native callbacks prove cancelled/native-closed editors cannot change draft or close a successor; a current Apply stages exactly once without Store writes. The model Settings save then proves the staged GeneralVideo row reaches persisted MediaViewerSettings and its MP4 lookup consumer. A retained Add chooser is invoked after parent retirement while a successor chooser is alive: no orphan is created and the successor remains visible. The successor's current Add stages a JPEG scale override, which parent cancellation leaves unpersisted. A separate test uses the actual `options_window::open` binding and checks that Options Apply stays open while the media editor is visible and succeeds after its native close.

Caveat: the first regression commits staged media values through the same Editor::applied/Settings::save boundary used by Options, rather than through the full main-window Options callback; the second regression exercises the actual Options callback's child visibility guard. These are focused lifetime/consumer checks, not a replacement for the media-view Python recording or screenshot suite.
