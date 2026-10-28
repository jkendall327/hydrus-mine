# Spike: the UI through Slint's interpreter (2026-10-27)

Question: does running the UI through Slint's interpreter, behind the same
typed Rust API, remove the build-time and binary-size cost of the generated
UI at an acceptable runtime cost? Branch `claude/spike-slint-interpreter`;
not for master. The owner decides on the numbers.

## The three numbers the decision hinges on

| | today (compiled) | interpreter (cached) |
|---|---|---|
| Whole GUI suite, wall time (1411 tests, 4-core machine) | **617 s** | **1297 s** (2.1x) |
| Whole GUI suite, peak RSS of the test process | **2.7 GB** | **8.9 GB** (fits 16 GB, ~7 GB spare) |
| `.slint` edit to a rebuilt GUI test binary: one dialog's file | **77 s** | **34 s** |
| `.slint` edit to a rebuilt GUI test binary: a shared file (`btn.slint`) | **376 s** | **34 s** |

- Suite: both runs with `MALLOC_ARENA_MAX=4` (gap 4 below). Without it the
  interpreter suite is OOM-killed at ~13.5 GB; the compiled one takes 573 s
  and 3.6 GB. 1414 of 1415 pass in both; the one failure is
  `emoji_fonts::outline_fox_...`, which AGENTS.md says fails in cloud
  containers. The interpreter suite also needs the `Bound` teardown fix
  (gap 3). Neither run uses a patched Slint.
- `.slint` edit: under the interpreter every `.slint` file costs the same,
  about what a Rust edit costs, because the only generated code is a 3.7 MB
  API file. Today the cost depends on how many of the six UI crates the
  file reaches.

The suite is 2.1x slower because each test thread compiles the windows it
opens (0.7-1.0 s for `main.slint` and its imports, 0.2-0.5 s for a dialog's
file), and libtest runs every test on a new thread. 1411 tests x ~1.5 s of
compiling / 4 threads is about the ~680 s difference.

## All numbers

Machine: cloud container, 4 cores, 15 GB, Rust 1.94.1, Slint 1.18.1, `-j4`.
The container restarted mid-spike onto a faster host (the compiled suite went
from 792 s to 573 s). Every comparison below pairs runs from the same host,
and says which. Memory is one of: peak system memory in use (sampled every
second), the largest single rustc's RSS, or the test process's RSS.

### Build (before the restart)

| | today | interpreter |
|---|---|---|
| clean build, deps included, UI + hydrus-gui lib and bin | 1420 s (UI crates 1108 + hydrus-gui 312) | 889 s |
| clean GUI test binary after that | 227 s | 203 s |
| peak rustc RSS, clean build | 4.7 GB (UI core crate) | 2.6 GB (3.4 GB linking the test binary; today 3.7 GB) |
| generated Rust | 87 MB in six crates | 3.7 MB in one |
| UI rlibs | 889 MB | 45 MB |
| debug binary `hydrus-gui` | 567 MB | 319 MB |
| release binary `hydrus-gui` (`debug = "line-tables-only"`) | ~900 MB (reported in the brief; not rebuilt here, for disk) | 462 MB, 98 MB stripped (interpreter optimised normally; clean release build 772 s, peak rustc 5.4 GB at LTO) |

### Incremental (after the restart)

| | today | cached |
|---|---|---|
| `.slint` edit, one dialog's file (`about.slint`): UI crates + test binary | 55 s + 23 s = 77 s | 31-34 s |
| `.slint` edit, a shared file (`btn.slint`) | 355 s + 21 s = 376 s | 33-34 s |
| Rust edit in hydrus-gui: test binary | 21 s | 31-36 s |
| Rust edit in hydrus-gui: lib and bin | 13 s | 21 s |
| peak rustc RSS, `.slint` edit | 2.3 GB (dialog) / 4.4 GB (shared) | 1.7 GB |

A Rust edit is ~10 s slower. My guess is that the live-preview API's `on_*`
handlers are generic, so each one is instantiated in hydrus-gui with `Value`
conversions in its body; I didn't check. (Untried: a dev mode that reads
`.slint` files from disk at run time would need no rebuild at all for an
edit that doesn't change the API.)

### Runtime (headless, software renderer; before the restart)

`ui_timing` (crates/hydrus-gui/tests/gui/ui_timing.rs):

| | today | live preview (stock) | cached |
|---|---|---|---|
| `MainWindow::new`, first on a thread | 25 ms | 4075 ms | 730-1040 ms (compiles main.slint) |
| `MainWindow::new`, again | 6 ms | 4774 ms | 10-13 ms |
| first `OptionsWindow::new` | 2.5 ms | 4392 ms | 460-530 ms (compiles options.slint) |
| first `ManageTagsWindow::new` | 6 ms | 4515 ms | 360-450 ms |
| first `AboutWindow::new`, then x20 | 0.8 ms, 3.7 ms | 5672 ms, 89 s | 220-250 ms, 5-7 ms |
| draw the main window (1100x700) | 130 ms | 174 ms | 110-120 ms |
| show and draw Options | 257 ms | 221 ms | 210-225 ms |

So app startup to a shown main window gains ~0.7-1.0 s (the main window's
compile), and each dialog's first opening gains 0.2-0.5 s.

Large store (`large_store::a_realistic`, 1000 files, from the steps it
prints):

| step | today | cached |
|---|---|---|
| open `system:inbox` | 314 ms | 355 ms |
| draw the grid | 901 ms | 873 ms |
| scroll to the middle / the end, drawn | 689 / 741 ms | 714 / 660 ms |
| select all | 271 ms | 289 ms |
| sort random | 588 ms | 607 ms |
| view item 0 (media viewer, first) | 954 ms | 1682 ms |
| view item 401 (viewer again) | 949 ms | 1663 ms |
| manage tags on item 0 | 3.0 s | 4.2 s |

Grid drawing and scrolling take the same time in both: the time is in the
renderer. The media viewer is ~1.7x slower even when it is already compiled;
it has many bindings, which the interpreter evaluates more slowly.

Test samples (before the restart): viewer (76 tests) 52 s against 92 s;
manage_tags (30) 30 s against 42 s; large_store realistic 32 s against 41 s.

## How the variants are built

Features on `generated/hydrus-gui-ui`, chosen through hydrus-gui:
`cargo test -p hydrus-gui --no-default-features --features interpreter-cached-ui`.

- **today** (`compiled`, default): the six generated crates, as on master.
- **live preview** (`interpreter-ui`): `generated/hydrus-gui-ui` alone. Its
  build.rs writes an entry file (`OUT_DIR/all.slint`: `export * from
  main.slint` plus every group file's `export { .. } from` lines, with
  absolute paths) and runs slint-build with `SLINT_LIVE_PREVIEW` set. It is
  one crate, not the module split, because the live-preview generator
  ignores library imports (`library_types_imports`): each group crate would
  define its own `Theme`, `TableRow` and so on. `@hydrus_ui_core` maps to
  `main.slint` through `with_library_paths`, so the types are the same
  everywhere. **hydrus-gui compiled against the generated API with no source
  changes.** Each `Window::new()` compiles `all.slint` from disk and starts a
  file watcher (stock `LiveReloadingComponent`): 4-5.7 s per window, which
  makes it unusable.
- **cached** (`interpreter-cached-ui`): the same API, rewritten by build.rs
  (string replacement on the generated code) to use
  `hydrus_gui_ui::interp::Component` (src/interp.rs):
  - every `.slint` file is embedded (`include_str!`, served through
    `Compiler::set_file_loader`);
  - a window's own file is compiled the first time a thread makes one of its
    windows, and cached per thread (interpreter definitions are `Rc`, not
    `Send`);
  - `new()` instantiates the cached `ComponentDefinition`, and there is no
    file watcher;
  - `HYDRUS_INTERP_WHOLE=1` compiles the whole UI at once instead
    (3.7-4.2 s).

## Gaps found (behaviour that differed, now handled in the spike)

1. **A global's API exists only if the entry file exports it.** The
   interpreter drops the properties and callbacks of globals the entry file
   doesn't export. With per-window entries, `Theme::retire-colours`,
   `TagTextHistory::record` and `Palette::color-scheme` were missing at run
   time, and 69 of 76 viewer tests failed. Each window's entry now re-exports
   every global.
2. **Weak handles died early.** The live-preview handle wraps the instance
   in its own `Rc`, so `as_weak()` stopped upgrading once Rust dropped its
   strong handles, even while Slint still showed the window. Compiled code
   keeps the component alive while shown, and callbacks holding weak handles
   rely on that (`headless_lifetime`, a file-menu test). The cached handle is
   now the interpreter's `ComponentInstance` and its `slint::Weak`.
3. **Leaked windows become expensive.** Each interpreter definition keeps
   the compiler's whole post-pass `TypeLoader`, a raw pre-pass snapshot of
   it, and debug info, all for the LSP and highlighting (slint-interpreter
   `component.rs`, `TypeLoaders`). So a leaked window instance keeps tens of
   MB alive, not the ~1 MB it costs today, and that made existing leaks
   fatal:
   - **Tests leaked dialogs; the app doesn't.** A dialog's callbacks hold the
     `Rc<RefCell<Option<Window>>>` slot that holds the dialog (for example
     `manage_tags_window.rs:72`, and `lib.rs` `Bound::viewer`). Closing the
     dialog empties the slot, so the app frees it: a probe
     (`ui_timing_closed_dialogs_are_freed`) shows that a closed manage tags
     window and media viewer are freed in both builds. A test that ends with
     one open kept it forever: about 7 MB per test compiled, about 90 MB
     under the interpreter. There's no bug to file on master.
   - The fix on this branch is `crates/hydrus-gui/src/bound_teardown.rs`, a
     `Drop` for `Bound`. On the final clone only, it hides and forgets the
     window in each of the 28 plain dialog slots; the predicate editor is
     left to its own owner. manage_tags (30 tests, one thread) now peaks at
     0.73 GB, against 2.9 GB before and 0.37 GB compiled.
   - 65 tests still leak, through windows in the other slot types
     (`*_window::Slots`: auto-resolution rules, formulae, choice buttons,
     import options, folders, database locations and others). The same
     teardown would cover them.
   - Also, `open_with` in some tests (`manage_tags_cog.rs`) drops its
     `headless::init()` collector immediately, against `headless::Windows`'
     own rule, so those windows are never hidden at teardown.
4. **Memory fragmentation.** With the leaks above fixed, the suite still
   grew to 13.5 GB with glibc's default arenas: four test threads each
   allocate ~0.5 GB transiently per compile. `MALLOC_ARENA_MAX=4` holds it at
   8.9 GB. One arena held it at ~2.5 GB but ran ~3x slower. CI's test step
   would set the variable.
5. **Per-thread compiles.** Interpreter definitions aren't `Send`, so nothing
   is shared between threads. Each test thread compiles what it opens, and so
   would each thread of the app that made windows (only the main thread
   does).

For comparison only, not proposed: a patched slint-interpreter
(vendor/slint-interpreter, `HYDRUS_INTERP_LEAN=1`: no debug info, no type
loaders kept, no raw snapshot). Before the leak fix, it cut manage_tags'
peak from 2.8 to 1.1 GB and `main.slint`'s compile from ~1.0 to ~0.7 s. A
Slint fork would be ours to carry; asking upstream for an option not to keep
the tooling state is the cheaper route.

## Risks

- **Unstable API.** The live-preview generator and `sp::live_preview` are
  Slint internals (`private_unstable_api`), and the cached variant rewrites
  their generated code by string replacement, so a Slint upgrade can break
  it. A real version would own its generator (the 505-line
  `rust_live_preview.rs`, adapted, in our build script or a small crate)
  rather than patch text. That is still not a Slint fork.
- **Two engines.** If the interpreter is used for development only, CI tests
  something other than what developers run. Gaps 1 and 2 are the kind of
  difference that would slip through. The suite passing under both is the
  evidence that they agree today; Slint documents no full parity.
- **Errors are still caught at build time.** build.rs compiles the whole UI
  with slint-build, so `.slint` errors and Rust/`.slint` type mismatches
  still fail the build. Only the per-window entries (re-exports, globals)
  are new at run time.
- **Shipping.** The `.slint` sources are embedded, but `@image-url` images
  (about 60 uses) are still read from their paths at run time, so shipping
  would need them embedded or installed. Startup and each dialog's first
  opening are slower (above), and the media viewer is ~1.7x slower to open
  and draw.
- **Test memory.** 8.9 GB of 16 GB leaves room, but the margin depends on
  leak hygiene in tests (gap 3) and on the arena setting, and it grows as
  the suite grows.

## Recommendation

The build side delivers what was hoped for:
- generated code goes from 87 MB to 3.7 MB, and UI rlibs from 889 MB to
  45 MB;
- a clean build is 37% faster, and peak compile memory falls from 4.7 to
  2.6 GB;
- any `.slint` edit costs ~34 s instead of 77-376 s;
- the six-crate split, `dev.sh ui` and the Slint memory rules in AGENTS.md
  would go.

The run side costs about as much again in tests. The suite takes 2.1x as
long (+11 minutes on this machine) and 3.3x the memory, mostly from
per-thread compiling, which libtest's thread-per-test makes unavoidable
without a shared compile. The app also starts ~1 s slower, and the viewer is
slower.

So it doesn't pay as a straight replacement, for CI or for shipping. Two
options are worth the owner's choice:

1. **Interpreter for local development only.** It would be a feature, as
   here, with compiled builds in CI and release. Developers get ~34 s
   `.slint` edits and builds that fit easily in memory, and CI keeps today's
   numbers. The cost is the two-engines risk above, and owning a small
   generator.
2. **Stay compiled**, and take the cheaper wins found on the way: the
   `Bound` teardown (frees ~7 MB per test today, worth landing on its own),
   and fixing the test files that drop their collector early.

If the owner picks option 1, the next steps are:
- own the generator instead of rewriting strings;
- embed the images;
- finish the teardown for the `*_window::Slots` types;
- add a weekly CI job that runs the suite under the interpreter, to catch
  divergence.

## Reproducing

- today: `scripts/dev.sh ui && cargo test -p hydrus-gui --test gui`
- interpreter: `cargo test -p hydrus-gui --test gui --no-default-features --features interpreter-cached-ui`.
  For the whole suite, set `MALLOC_ARENA_MAX=4`. `HYDRUS_INTERP_TIMING=1`
  prints each compile and the RSS; `HYDRUS_INTERP_LEAKS=1` reports windows
  still alive when a test thread ends.
- timings: `ui_timing` (`-- ui_timing --nocapture`), and
  `large_store::a_realistic` with `--nocapture`.
