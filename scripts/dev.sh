#!/usr/bin/env bash
# The local development loop. Fast enough to run after every change.
#
#   scripts/dev.sh slint            compile the .slint files only (~10 s)
#   scripts/dev.sh ui               build the generated UI crate on its own
#                                   (no-op when fresh; ~7 min after a .slint edit)
#   scripts/dev.sh gui [FILTER...]  GUI tests whose names match a filter, e.g.
#                                   `export_files::` (~20 s after a Rust edit)
#   scripts/dev.sh model [FILTER]   hydrus-gui-model tests matching FILTER
#   scripts/dev.sh test CRATE...    every test of the named crates
#   scripts/dev.sh lint [CRATE...]  rustfmt (applied) and strict Clippy on the named
#                                   crates (default: crates changed vs origin/master)
#   scripts/dev.sh pre-push         lint the changed crates, slint check, test the
#                                   changed crates; the GUI crate's whole suite
#                                   if it (or anything it uses) changed
#
# Why the UI crate is built on its own: it is ~87 MB of generated Rust and
# rustc peaks near 13 GB compiling it. Cargo starts compiling hydrus-gui while
# the UI crate's code generation is still running, and the two together do not
# fit in 16 GB. Building it first, alone, avoids the overlap. The
# hydrus-workspace-hack crate makes that solo build resolve the same features
# as the full graph, so the artifact is reused.
#
# Environment: DEV_JOBS (default 4) is the Cargo job count. DEV_LANE names a
# GUI test lane (crates/hydrus-gui/tests/lane_<name>.rs) for `gui` and `lint`
# to use instead of the shared `gui` test binary (see AGENTS.md, "Several
# agents at once").
set -euo pipefail
cd "$(dirname "$0")/.."

jobs=${DEV_JOBS:-4}
gui_test=${DEV_LANE:+lane_$DEV_LANE}
gui_test=${gui_test:-gui}
cargo_() { cargo "$1" --locked -j "$jobs" "${@:2}"; }

slint_check() {
  cargo run -q --manifest-path tools/slint-check/Cargo.toml --target-dir target/tools
}

build_ui() {
  cargo_ build -p hydrus-gui-ui
}

changed_crates() {
  local base
  base=$(git merge-base HEAD origin/master 2>/dev/null || git rev-parse HEAD)
  { git diff --name-only "$base"; git ls-files --others --exclude-standard; git diff --name-only; } |
    sed -n 's|^crates/\([^/]*\)/.*|\1|p' | sort -u | grep -v '^hydrus-gui-ui$' || true
}

# Crates whose change can break hydrus-gui (everything it depends on).
gui_inputs='hydrus-core hydrus-store hydrus-parse hydrus-net hydrus-download hydrus-downloader-exchange hydrus-search hydrus-duplicates hydrus-media hydrus-import hydrus-gui-model hydrus-gui hydrus-workspace-hack'

lint() {
  local crates=("$@")
  if [ ${#crates[@]} -eq 0 ]; then
    mapfile -t crates < <(changed_crates)
  fi
  cargo fmt --all
  if [ ${#crates[@]} -eq 0 ]; then
    echo "lint: no changed crates"
    return
  fi
  local args=() gui=0
  for c in "${crates[@]}"; do
    if [ "$c" = hydrus-gui ] && [ -n "${DEV_LANE:-}" ]; then gui=1; else args+=(-p "$c"); fi
  done
  if [ ${#args[@]} -gt 0 ]; then cargo_ clippy "${args[@]}" --all-targets -- -D warnings; fi
  # (in a lane, lint hydrus-gui's library and only that lane's tests, so other
  # agents' unfinished lanes cannot fail this)
  if [ $gui = 1 ]; then
    cargo_ clippy -p hydrus-gui --lib --bins --test "$gui_test" -- -D warnings
  fi
}

case "${1:-}" in
  slint) slint_check ;;
  ui) build_ui ;;
  gui)
    shift
    build_ui
    if [ $# -eq 0 ]; then
      cargo_ test -p hydrus-gui --test "$gui_test"
    else
      for f in "$@"; do cargo_ test -p hydrus-gui --test "$gui_test" -- "$f"; done
    fi
    ;;
  model)
    shift
    cargo_ test -p hydrus-gui-model --test model -- "$@"
    ;;
  test)
    shift
    for c in "$@"; do
      [ "$c" = hydrus-gui ] && build_ui
      cargo_ test -p "$c"
    done
    ;;
  lint)
    shift
    lint "$@"
    ;;
  pre-push)
    mapfile -t crates < <(changed_crates)
    git diff --name-only "$(git merge-base HEAD origin/master 2>/dev/null || echo HEAD)" -- crates/hydrus-gui/ui | grep -q . && slint_check
    gui=0
    for c in "${crates[@]}"; do
      case " $gui_inputs " in *" $c "*) gui=1 ;; esac
    done
    [ $gui = 1 ] && build_ui
    lint "${crates[@]}"
    for c in "${crates[@]}"; do
      [ "$c" = hydrus-gui ] && continue
      cargo_ test -p "$c"
    done
    if [ $gui = 1 ]; then cargo_ test -p hydrus-gui; fi
    ;;
  *)
    sed -n '2,25p' "$0"
    exit 2
    ;;
esac
