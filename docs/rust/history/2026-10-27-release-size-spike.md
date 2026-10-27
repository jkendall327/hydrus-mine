# Release binary size spike (2026-10-27)

Question: the Linux release `hydrus-gui` is ~900 MB. What shrinks it, within a
16 GB machine (GitHub's runners, `.github/workflows/release.yml`)?

Measured on a 4-core, 15 GB container, Rust 1.94.1, master `0803d63`, building
in the order `release.yml` uses (UI crates one at a time, core first, then the
facade, then `hydrus-cli` and `hydrus-gui`). Peak memory is the largest resident
set of any process in the step (`wait4`'s `ru_maxrss`).

## What was big

The binary was mostly debug info, not code:

| Section of the baseline `hydrus-gui` (916 MB) | Size |
|---|---|
| `.debug_info` | 328 MB |
| `.debug_str` | 155 MB |
| `.debug_ranges` | 109 MB |
| `.debug_line` | 93 MB |
| `.debug_loc` | 11 MB |
| `.text` | 122 MB |
| `.rodata` | 13 MB |

`debug = "line-tables-only"` still leaves ~700 MB of DWARF once thin LTO has
inlined everything. Of the 122 MB of code, the generated Slint UI is ~38 MB
(`nm --size-sort`), `core`/`alloc` ~27 MB, `i_slint_core` ~8 MB.

**The premise "the UI is built at opt-level 0" was wrong.** The only override was
`[profile.release.package.hydrus-gui-ui]`, which is the facade. The six real
crates (`-core`, `-options`, `-dialogs-1..4`) were built at the release
default, `opt-level = 3`, and `rustc` for `-core` showed `-C opt-level=3`.
Baseline peaks were already within 16 GB: core 6.7 GB, final link 6.9 GB.

## Results

All rows: `hydrus-gui` / `hydrus` size, peak RSS of the worst UI crate (core)
and of the final `hydrus-cli` + `hydrus-gui` build. Times are wall clock; the
core row of a from-scratch build includes dependencies, so compare the
"final" column for time.

| # | Profile | hydrus-gui | hydrus | core peak | final peak | final time |
|---|---|---|---|---|---|---|
| 0 | baseline (line tables, no strip, UI at 3) | 916 MB | 239 MB | 6.7 GB | 6.9 GB | 953 s |
| 0a | baseline, then `strip --strip-debug` | 211 MB | 61 MB | - | - | - |
| 0b | baseline, then full `strip` (no symbol names) | 166 MB | - | - | - | - |
| 1 | `debug = false`, `strip = "debuginfo"` | 213.5 MB | 61 MB | 5.7 GB | 3.6 GB | 825 s |
| 2 | line tables kept, split with `objcopy` (see below) | 211 MB + 747 MB `.debug` | 61 MB + 188 MB | 6.6 GB | 6.8 GB | 973 s |
| 3 | 1 + UI crates at `opt-level = "s"` | 188 MB | 61 MB | 5.5 GB | 3.5 GB | 797 s |
| 4 | 1 + UI crates at `opt-level = "z"` | 189 MB | 61 MB | 5.5 GB | 3.5 GB | 673 s |

Per-crate peaks and times for the UI crates, experiment 3 vs baseline:

| Crate | baseline | opt-level s |
|---|---|---|
| core | 6.7 GB | 5.5 GB |
| dialogs-1 | 4.7 GB, 145 s | 3.8 GB, 136 s |
| dialogs-3 | 4.1 GB, 118 s | 3.3 GB, 110 s |
| options | 3.7 GB, 97 s | 2.9 GB, 94 s |
| dialogs-2 | 3.5 GB, 91 s | 2.7 GB, 85 s |
| dialogs-4 | 3.4 GB, 94 s | 2.7 GB, 86 s |

Every crate and the final link stay under 7 GB in all experiments, so the
~12 GB budget is not a constraint.

Findings:

- **Dropping debug info is the whole win** (916 -> ~213 MB). It also cuts the
  final link's peak from 6.9 GB to 3.6 GB.
- `strip` is a per-crate rustc flag, so changing `debug` or `strip` rebuilds
  every crate (~45 minutes here from scratch). Stripping the finished binary
  with `strip` gives the same size without a rebuild.
- `opt-level = "s"` on the UI crates saves a further ~25 MB (12%). `"z"` is no
  smaller, so use `"s"`. Release code that the user runs is mostly the Slint
  runtime (`i_slint_core`, not affected), not the generated bindings.
- Keeping symbol names (`strip = "debuginfo"` or `strip --strip-debug`) costs
  ~45 MB over a full `strip` and keeps function names in backtraces.
- Not tried: `lto = "fat"`, `codegen-units = 1`, and `opt-level = "s"` for the
  whole workspace. Per-package `lto` is not settable in Cargo; the UI crates
  are compiled for linker-plugin LTO and the final crate's settings decide the
  link, so their `codegen-units` is not the lever.

## Backtraces keep line numbers

A stripped binary prints function names but no `file:line` in backtraces. The
panic header (`panicked at file:line:col`) is always there. To keep line numbers:

```
objcopy --only-keep-debug hydrus-gui hydrus-gui.debug
strip --strip-debug hydrus-gui
objcopy --add-gnu-debuglink=hydrus-gui.debug hydrus-gui
```

Checked on a real panic in the experiment-2 `hydrus-gui` (this container lacks
`libxkbcommon-x11`, so the binary panics on start):

| Setup | Backtrace frame |
|---|---|
| stripped binary alone | `xkbcommon_dl::x11::xkbcommon_x11_handle` |
| `.debug` file next to the binary | the same, plus `at .../xkbcommon-dl-0.4.2/src/x11.rs:59:28` |
| zlib-compressed `.debug` next to the binary | the same, with `file:line` |

Sizes of the `.debug` file: 747 MB for `hydrus-gui` (150 MB as `.tgz`), 188 MB for
`hydrus` (45 MB). `split-debuginfo = "packed"` was not tried: on Linux it
produces `.dwp` files, which std's backtrace does not read.

## What changed

- `Cargo.toml`: `debug = false`, `strip = "debuginfo"` for release; the six UI
  crates at `opt-level = "s"`; the stale comment about opt-level 0 corrected.
- `release.yml`: the Linux job builds with line tables and no strip, splits
  them into `hydrus.debug` / `hydrus-gui.debug` (with a debuglink), and uploads
  a `-debug` archive next to the binary archive. Windows is built with the
  profile's own settings.

The new release workflow has only been run here as the same commands, not on a
GitHub runner; the owner must dispatch it to prove it there.

## Not verified

- No window was opened: the container has no `libxkbcommon-x11`. The binaries
  start, open the store, and reach winit's X11 setup.
