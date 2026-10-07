# Namespace observer structure repair: source review

No remaining concrete source blocker found in the corrected wrapper. The Apply Button and measurement Timer are now siblings in one conditional `HorizontalLayout`, with zero spacing. The Timer still reports the **actual Button** geometry.

The earlier review missed Button’s prohibition on children; CI80b caught it. The earlier report remains unchanged as historical source-only evidence. This repair report supersedes that initial syntax assessment and grants no source, runtime/render or claim approval.

Baseline `80bcb531f7df156e448cc212254f941541101dc3`; repaired session_dialog SHA256 `e0e95b66f749c57ecae8b74597345704e9e9a793304c2fa1807327673d011518`; selected diff SHA256 `3a85acf0d5ea18ce42f85cf9b8f0f40c819a8e919a7f73d0639696ebb6a33df0`. Only this Slint file is dirty.

## Button/Timer scope

Button contains only enabled/text and clicked callback. Timer is a sibling and can reference name-apply in the same asking-name conditional layout. The name-apply-measured callback still returns actual Button absolute x/y/width/height, not wrapper geometry. Timer is nonvisual and adds no visible control.

Source: `crates/hydrus-gui/ui/session_dialog.slint:110-129`.

## Layout invariants

Wrapper contains one visual Button with spacing0 and no introduced padding/size binding. Outer footer still alignment end/spacing6; Cancel remains its sibling. No caption, conditional visibility, enabled child-open interlock, blank guard, callback routing or keyboard focus/input frame changes. Actual allocated button/footer geometry remains a runtime/render gate; source inspection does not prove pixel equality.

Source: `crates/hydrus-gui/ui/session_dialog.slint:99-135`.

## Default-off observation and semantic preservation

The existing measure-name-apply bool still defaults false; the sibling Timer is conditional on asking-name and only runs with that opt-in. Exact-empty widget guard and nonempty/name_entered routes are unchanged. SessionDialog shared notice/question paths and their focus handling are untouched.

Source: `crates/hydrus-gui/ui/session_dialog.slint:31-35,104-109,115-128`.

## Tests and Rust unchanged

Namespace Rust owner implementation and full GUI test file are byte-identical to80b; all107 current assertion macros, including all53 original baseline assertions, are retained. Actual measured pointer test adapts to actual Button geometry and requires no test relaxation or coordinate substitution.

Source: `crates/hydrus-gui/tests/gui/namespace_colours.rs`.

## Supplied generation evidence

Read root’s `/workspace/validation-reviews/namespace2/slint-generation-check/result.json`: cached Slint 1.18.1 generation-only exit 0, elapsed 8.059107234992553 s, empty diagnostics. This reviewer ran no build. The result does not contain its source SHA; the repaired file is pinned separately above. Rust compilation, strict lint, tests and actual image acceptance are unproven by this generation check.

Namespace owner Rust and test files are byte-identical to80b; all **107** current assertions, including all **53** original assertions, are retained.

## Required next gates

- Commit repaired exact source and obtain full Linux compilation, strict Clippy, full GUI/workspace tests and parity success.
- Actually inspect fresh Add/Delete/blank-warning/retained-input/whitespace PNGs from that exact artifact; no pixel/modal/focus approval from source or generation-only proof.
- Retain Add/Delete-only scope, zero parent/alias/Namespace Edit credit and deferred macOS/Windows.
