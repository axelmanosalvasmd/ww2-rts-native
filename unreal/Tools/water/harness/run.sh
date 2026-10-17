#!/usr/bin/env bash
# Builds the standalone harness, runs it on every case the oracle wrote and compares.
#   run.sh <oracle dir> [<build dir>] [<source dir>]   (build dir default: <oracle dir>/build)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ORACLE="${1:?oracle dir (oracle.mjs --out)}"; BUILD="${2:-$ORACLE/build}"; SRC="${3:-$HERE/../../../Source/ThreeCrossroads}"
mkdir -p "$BUILD"
CXX="${CXX:-g++}"
"$CXX" -std=c++17 -O2 -Wall -Wextra -Wno-unused-parameter -I"$SRC" "$HERE/water-harness.cpp" -o "$BUILD/water-harness"
CASES=$(cd "$ORACLE" && find . -mindepth 2 -maxdepth 2 -name input.txt | sed 's|^\./||; s|/input.txt$||' | sort)
"$BUILD/water-harness" "$ORACLE" $CASES
node "$HERE/compare.mjs" "$ORACLE" "$BUILD/compare.json"
