# Spike: the UI through Slint's interpreter (2026-10-27)

Question: does running the UI through Slint's interpreter, behind the same
typed Rust API, remove the build-time and binary-size cost of the generated
UI at an acceptable runtime cost? Branch `claude/spike-slint-interpreter`;
not for master.

Machine: cloud container, 4 cores, 15 GB, Rust 1.94.1, Slint 1.18.1.
`-j4` throughout. Memory figures: peak system memory in use (sampled every
second) and the largest single rustc's RSS.

## How the variants are built

- **today** (`compiled`, default): the six generated crates, as on master.
- **live preview** (`hydrus-gui --no-default-features --features interpreter-ui`):
  `generated/hydrus-gui-ui` alone, its build.rs writing an entry file
  (`OUT_DIR/all.slint`: `export * from main.slint` plus every group file's
  `export { .. } from` lines, with absolute paths) and running slint-build
  with `SLINT_LIVE_PREVIEW` set. One crate, not the module split: the
  live-preview generator ignores library imports (`library_types_imports`),
  so each group crate would define its own copy of `Theme`, `TableRow`...
  `@hydrus_ui_core` maps to `main.slint` through `with_library_paths`, so
  the types are the same everywhere. Each `Window::new()` compiles
  `all.slint` from disk with the interpreter and starts a file watcher
  (stock `LiveReloadingComponent`).
- **cached** (`--features interpreter-cached-ui`): the same generated API,
  rewritten by build.rs to use `hydrus_gui_ui::interp::Component`
  (src/interp.rs): every `.slint` file is embedded (`include_str!`, served
  through `Compiler::set_file_loader`), the whole UI is compiled once per
  thread (interpreter definitions are `Rc`, not `Send`) and cached, and
  each `new()` instantiates the cached `ComponentDefinition`. No watcher.

## Numbers

(BASELINE_TABLE)

## Gaps found

(GAPS)

## Risks

(RISKS)

## Recommendation

(RECOMMENDATION)
