#!/usr/bin/env bash
# CI's checks, run locally before pushing: rustfmt, workspace clippy with
# warnings denied, tests, and the parity ratchet. Prints one line per step
# and exits non-zero if any failed.
#
#   scripts/check.sh                  every crate's tests, as CI runs them
#   scripts/check.sh hydrus-gui ...   only these crates' tests (enough for a
#                                     change to them alone; a change to a
#                                     shared crate such as hydrus-core or
#                                     hydrus-store needs the full run)
#
# HYDRUS_CHECK_LOW_DISK=1 deletes each crate's test executables after its
# tests (they are hundreds of megabytes each).
# HYDRUS_CHECK_KEEP_GUI=1 retains GUI test executables during that cleanup,
# so a targeted GUI run can be reused by the final workspace check.
set -u
cd "$(dirname "$0")/.."

failed=0
step() {
  local name=$1
  shift
  if "$@" > "target/check-$name.log" 2>&1; then
    echo "ok      $name"
  else
    echo "FAILED  $name (see target/check-$name.log)"
    failed=1
  fi
}
clean_executables() {
  if [ "${HYDRUS_CHECK_LOW_DISK:-0}" = 1 ]; then
    if [ "${HYDRUS_CHECK_KEEP_GUI:-0}" = 1 ]; then
      find target/debug/deps -maxdepth 1 -type f -executable ! -name "*.so" ! -name "*.rlib" ! -name "gui-*" ! -name "hydrus_gui-*" -delete
    else
      find target/debug/deps -maxdepth 1 -type f -executable ! -name "*.so" ! -name "*.rlib" -delete
    fi
  fi
}
mkdir -p target

step fmt cargo fmt --all --check
# (CI builds --locked from what is committed)
if ! git diff --quiet HEAD -- Cargo.lock; then
  echo "FAILED  Cargo.lock changed but is not committed"
  failed=1
fi
clippy() {
  cargo clippy -q --workspace --all-targets --locked -- -D warnings
}
step clippy clippy

if [ $# -gt 0 ]; then
  packages=("$@")
else
  packages=()
  for dir in crates/*/; do
    packages+=("$(basename "$dir")")
  done
fi
for pkg in "${packages[@]}"; do
  step "test-$pkg" cargo test -q -p "$pkg" --locked --no-fail-fast
  clean_executables
done

step ratchet cargo xtask ratchet --check
exit $failed
