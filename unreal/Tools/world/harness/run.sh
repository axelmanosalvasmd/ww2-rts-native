#!/usr/bin/env bash
# Builds the standalone harness, runs it on the oracle fixture's inputs and compares. Scratch dir: $1 (required; e.g.
# <scratch>/slice11-world/harness). Source dir: $2 (default: this repo's unreal/Source/ThreeCrossroads).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="${1:?scratch dir}"; SRC="${2:-$HERE/../../../Source/ThreeCrossroads}"
FIX="${FIXTURE:-$HERE/../fixtures/scenery-oracle.json.gz}"
mkdir -p "$OUT"
CXX="${CXX:-g++}"
"$CXX" -std=c++17 -O2 -Wall -Wextra -Wno-unused-parameter -I"$SRC" "$HERE/scenery-harness.cpp" -o "$OUT/scenery-harness"
node "$HERE/to-input.mjs" "$FIX" "$OUT/input.txt"
"$OUT/scenery-harness" "$OUT/input.txt" "$OUT/output.txt"
node "$HERE/compare.mjs" "$FIX" "$OUT/output.txt"
