# Namespace Add/Delete2: independent source review

No remaining concrete syntax/API, Rc ownership/reentrancy or actual-input sequence blocker found in the latest dirty snapshot. No source, runtime, rendered or claim approval is issued.

Baseline `b87f0e2af634e8ec7ae68bfb2f01e14143d76758`; product/test diff SHA256 `30db78f9a37e2b45152696246a4915045e4c3a66c29930835f8eeea53a7b5043`. Individual reviewed dirty file hashes and actual reference fixture/script hashes are recorded in JSON. Root alone performed edits and supplied the real Qt recorder execution.

Resolved during review: the new blank callback now refuses its own hidden input even when Options remains visible. Its new hidden-input regression preserves the original direct-handler fixture semantics.

## Slint/API/type inference

No concrete Slint syntax or Rust API/type blocker identified by source inspection. Conditional named Button items and Timer children follow existing Slint patterns. MenuChoiceFrame is already exported; name-input-frame binds directly to structural LineEdit geometry and name-apply-measured emits that struct, avoiding destructive two-way measurement links. Rc<RefCell<Option<_>>> is constrained by the generated typed callback assignment; frame fields and Clone usage match the existing generated geometry struct. ComponentHandle and WindowAdapter imports supply the respective window APIs. No compilation was run.

Source: `crates/hydrus-gui/ui/session_dialog.slint:10,31-35,110-120`, `crates/hydrus-gui/ui/viewer.slint:1094`, `crates/hydrus-gui/tests/gui/namespace_colours.rs:585,642-666`.

## Exact-empty widget guard versus original handler fixtures

Bool opt-ins default false and only Namespace Add enables reject-blank-submission. Actual LineEdit.accepted and Apply.clicked test exact empty string; whitespace reaches the existing handler. Direct invoke_name_entered remains the original handler path, subject only to occupied nested-warning admission, so original empty/reserved/duplicate/cancel fixture callbacks retain their meaning. Apply remains enabled for empty input, matching Qt veto rather than silently disabling acceptance. Shared LineEdit accepted additionally obeys child-open, matching its enabled-state interlock. Delete now uses Are you sure? while Add retains Enter Text.

Source: `crates/hydrus-gui/ui/session_dialog.slint:31,82-91,110-128`, `crates/hydrus-gui/src/options_namespace_colours.rs:245-258,320-349`, `hydrus/client/gui/panels/ClientGUIScrolledPanelsTextEntry.py:109-134`, `hydrus/client/gui/ClientGUIDialogsQuick.py:42`, `hydrus/client/gui/panels/options/TagPresentationPanel.py:243-265`.

## Nested notice Rc ownership and reentrancy

Primary child remains the original input. Per-input entry_warning holds the nested notice; notice callbacks retain only weak slot/input back-edges through sync, so no new primary-input/notice Rc cycle was identified. Input close first marks child_active=false, takes the nested notice into a local with RefCell borrow released at the semicolon, then invokes cancellation. Notice close is one-shot, hides itself, clears only its own private slot, reenables entry and refreshes parent. Stale input/notice callbacks cannot clear a successor after retirement because their one-shot cells return before slot mutation. Warning-show failure cleans the notice and reports parent error. No borrow is kept while invoking the nested close/sync callbacks.

Source: `crates/hydrus-gui/src/options_namespace_colours.rs:18-55,259-278,280-319`, `crates/hydrus-gui/src/options_window.rs:735-755`.

## Hidden entry admission resolved during review

Initial new blank callback admitted a hidden but still-active input when Options remained visible. Root added a weak entry visibility guard specifically to the new callback; this leaves original direct handler fixture semantics unchanged. New regression hides entry, invokes blank callback, asserts no new window/child-open, then shows it and exercises normal real Return. This is resolved in the reviewed source, with execution pending.

Source: `crates/hydrus-gui/src/options_namespace_colours.rs:285,305-309`, `crates/hydrus-gui/tests/gui/namespace_colours.rs:885-896`.

## Actual input/ACK sequencing and fixture evidence

New test reads actual namespace_entry_validation fixture, asserting recorder errors empty, title/prompt and allow_blank false. It measures real Apply geometry with bounded draw/timer loop, uses physical pointer events or Return for exact empty submission, checks one new notice and retained original input identity, unchanged draft/DB, blocked retained acceptance and pending parent Apply. Notice Return acknowledgement is followed by explicit real window activation and a real pointer click into the reenabled LineEdit before actual typing; this proves renewed editing and same-input retention, not automatic OS focus restoration. Accepted input and staged namespace are checked against recorded Qt rows; parent Cancel/reopen restores original rows. Physical whitespace separately forwards to handler, closes input and verifies original reserved warning title/message/OK with unchanged DB. Parent Cancel retires nested warning/input; retained callbacks then leave current successor visible, unblocked and in its slot.

Source: `crates/hydrus-gui/tests/gui/namespace_colours.rs:611-623,631-791,797-936`, `oracle/record_namespace_entry_validation.py`, `oracle/fixtures/namespace_entry_validation.json`, `crates/hydrus-gui/ui/session_dialog.slint:53-61`.

## Original assertions and defining captures

All53 original assertion macros remain unmodified; the only removed original test line is a headless render now stored in pixels for the same real Add entry PNG. Existing business-handler fixture replay, Cancel/Apply/reopen, invalid/duplicate warnings, hidden parent/retired successor, selections and live colour/OR consumers remain. New Delete question asserts title/type and snapshots declined state; new actual empty warning/retained entry and whitespace captures use real owner adapters. No native PNGs were produced or viewed by this source review; exact-source full Linux CI and fresh inspection remain gates.

Source: `crates/hydrus-gui/tests/gui/namespace_colours.rs:156-167,278-294,683-692,739-751,862-872`.

Original assertions: all **53** remain; current macro count **107**. New geometry/keyboard/ownership assertions are pending execution.

## Limits

- Only Namespace Add/Delete entry, warning, staging/Cancel/persistence boundaries are reviewed; Namespace Edit lifetime, structural parents, aliases and global backend parity receive no credit.
- Direct handler fixture replay deliberately differs from real exact-empty widget veto. Preserve both evidence paths without attributing mocked EnterText empty behavior to actual Qt dialog acceptance.
- Nested native notice is explicitly owned by input via a private slot; native OS modality/title chrome and automatic focus restoration are not established by headless source review.
- New native tests use real pointer/keyboard routes and explicitly click restored LineEdit after acknowledgement; do not claim automatic focus return from that sequence.
- Qt real-widget fixture/PNG execution is supplied by root/reference reviewer; this reviewer checked current source/JSON/hash contracts, without rerunning or inspecting PNGs.
- Exact committed-source full Linux compilation/strict lint/tests and independent fresh native-vs-Qt visual review remain mandatory; macOS/Windows deferred.
