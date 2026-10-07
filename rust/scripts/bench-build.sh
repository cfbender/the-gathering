#!/usr/bin/env bash
# Measures compile times for the dev loop: a cold build, a one-line edit then build, and a
# one-line edit then check, plus the same edit then `cargo test --no-run`.
#
#   bash rust/scripts/bench-build.sh [label]
#
# Builds into a scratch target directory (BENCH_TARGET, default /tmp/the-gathering-bench) so the
# normal one is untouched, and edits rust/crates/the-gathering/src/games/color_identity.rs (or
# BENCH_FILE) by appending a constant, which it removes again at the end. Prints one line per
# step with wall-clock seconds; record the results in rust/notes/compile-times.md.
set -euo pipefail
rust="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$rust"
label="${1:-$(git rev-parse --short HEAD)}"
export CARGO_TARGET_DIR="${BENCH_TARGET:-/tmp/the-gathering-bench}"
file="${BENCH_FILE:-$rust/crates/the-gathering/src/games/color_identity.rs}"
cp "$file" "$file.bench-orig"
trap 'mv "$file.bench-orig" "$file"' EXIT

step() {
  local name="$1"
  shift
  local started tenths
  started="$(date +%s%N)"
  "$@" >/dev/null 2>&1 || { echo "$name failed" >&2; "$@" 2>&1 | tail -20 >&2; exit 1; }
  tenths=$((($(date +%s%N) - started) / 100000000))
  printf '%-28s %4d.%ds\n' "$name" $((tenths / 10)) $((tenths % 10))
}
edit() {
  printf '\nconst _BENCH_%s: u32 = %s;\n' "$1" "$1" >>"$file"
}

linker="$(command -v mold >/dev/null && [[ "${THE_GATHERING_LINKER:-}" != default ]] && echo mold || echo default)"
echo "== $label, $(nproc) cores, CARGO_INCREMENTAL=${CARGO_INCREMENTAL:-default}, linker $linker, target $CARGO_TARGET_DIR"
rm -rf "$CARGO_TARGET_DIR"
step "cold build" cargo build --locked --bin the-gathering
edit 1
step "edit, build" cargo build --locked --bin the-gathering
step "cold check" cargo check --locked --bin the-gathering
edit 2
step "edit, check" cargo check --locked --bin the-gathering
step "tests --no-run (warm-up)" cargo test --locked --no-run
edit 3
step "edit, tests --no-run" cargo test --locked --no-run
if [[ "$linker" == mold ]]; then
  edit 4
  THE_GATHERING_LINKER=default step "edit, tests (GNU ld)" cargo test --locked --no-run
fi
du -sh "$CARGO_TARGET_DIR" | awk '{print "target size                  " $1}'
