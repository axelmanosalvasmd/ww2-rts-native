#!/usr/bin/env bash
# Builds the standalone harness on the engine-free painter (Source/ThreeCrossroads/World/TCGroundPaint.h and the shipped
# tiles, TCGroundPaint.Tiles.inl), runs it on a ground-oracle.mjs output and compares the overlays and decals.
#   run.sh <oracle dir> <scratch dir> [<make-ground-tiles out with --cases canvases>]
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ORACLE="${1:?oracle dir}"; OUT="${2:?scratch dir}"; TILES="${3:-}"
SRC="$HERE/../../../Source/ThreeCrossroads"
mkdir -p "$OUT"
"${CXX:-g++}" -std=c++17 -O2 -ffp-contract=off -Wall -Wextra -Wno-unused-parameter -DTC_GROUND_PAINT_CORE_ONLY -I"$SRC" "$HERE/ground-harness.cpp" -o "$OUT/ground-harness"
if [ -n "$TILES" ]; then "$OUT/ground-harness" "$ORACLE" "$OUT" --tiles "$TILES"; else "$OUT/ground-harness" "$ORACLE" "$OUT"; fi
node "$HERE/compare.mjs" "$ORACLE" "$OUT"
