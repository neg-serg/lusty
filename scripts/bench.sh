#!/usr/bin/env bash
# Listing benchmark for the picker. Uses hyperfine when available, otherwise a
# plain millisecond loop. Environment:
#   BENCH_ROOTS  space-separated roots (default: the repo and /etc/nixos)
#   BENCH_DEPTH  listing depth (default 2)
#   BENCH_RUNS   hyperfine runs (default 10)
# The huge /nix/store tree is opt-in: BENCH_ROOTS=/nix/store scripts/bench.sh
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel 2> /dev/null || pwd)"
cd "$repo_root"

cargo build --release > /dev/null
bin="$repo_root/target/release/lusty"
depth="${BENCH_DEPTH:-2}"
roots="${BENCH_ROOTS:-$repo_root /etc/nixos}"

echo "lusty --list, depth $depth"
if command -v hyperfine > /dev/null 2>&1; then
  hyperfine --warmup 2 --runs "${BENCH_RUNS:-10}" -N -L root "$roots" \
    "$bin --list {root} --depth $depth"
else
  for root in $roots; do
    t0=$(date +%s%N)
    "$bin" --list "$root" --depth "$depth" > /dev/null 2>&1 || true
    t1=$(date +%s%N)
    printf '  %-24s %s ms\n' "$root" "$(( (t1 - t0) / 1000000 ))"
  done
fi
