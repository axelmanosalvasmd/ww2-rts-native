#!/usr/bin/env bash
# Builds the standalone harness, runs it on the oracle fixture's inputs and compares. Scratch dir: $1 (required).
# Source dir: $2 (default: this repo's unreal/Source/ThreeCrossroads).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="${1:?scratch dir}"; SRC="${2:-$HERE/../../../Source/ThreeCrossroads}"
FIX="${FIXTURE:-$HERE/../fixtures/atmos-oracle.json.gz}"
mkdir -p "$OUT"
CXX="${CXX:-g++}"
"$CXX" -std=c++17 -O2 -Wall -Wextra -Wno-unused-parameter -I"$SRC" "$HERE/atmos-harness.cpp" -o "$OUT/atmos-harness"
node "$HERE/to-input.mjs" "$FIX" "$OUT/input.txt"
"$OUT/atmos-harness" "$OUT/input.txt" "$OUT/output.json"
node "$HERE/compare.mjs" "$FIX" "$OUT/output.json"
