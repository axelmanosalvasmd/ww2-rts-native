#!/usr/bin/env bash
# Builds the engine-free relief harness and runs it on every oracle case, then compares (README.md).
#   run.sh <oracle dir> <work dir> [case...]
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); SRC=$(cd "$HERE/../../../Source/ThreeCrossroads" && pwd)
ORACLE=$1; WORK=$2; shift 2
mkdir -p "$WORK"
g++ -std=c++17 -O2 -ffp-contract=off -Wall -Wextra -Wshadow -I"$SRC" "$HERE/relief-harness.cpp" "$SRC/World/TCRelief.cpp" -o "$WORK/relief-harness"
cases=("$@")
if [ ${#cases[@]} -eq 0 ]; then mapfile -t cases < <(ls "$ORACLE" | grep '\.json$' | grep -v '^fixture.json$' | sed 's/\.json$//'); fi
for c in "${cases[@]}"; do
  node "$HERE/to-input.mjs" "$ORACLE" "$c" "$WORK/$c.in"
  echo "== $c"; "$WORK/relief-harness" "$WORK/$c.in" "$ORACLE" "$WORK" "$c"
done
node "$HERE/compare.mjs" "$ORACLE" "$WORK" "$WORK/summary.json"
