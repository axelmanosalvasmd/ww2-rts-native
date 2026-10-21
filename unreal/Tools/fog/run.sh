#!/usr/bin/env bash
# The fog of war and haze checks against the pinned reference (README.md). usage: unreal/Tools/fog/run.sh <reference copy>
#   <reference copy>: a git-archive copy of reference/ww2-rts with node_modules (tc-local.sh serve makes one at
#   <TC_RUN_DIR>/ref, with ref.commit beside it). Writes into <this repository>/unreal/Saved/fog-oracle.
set -e
REF=$(cd "$1" && pwd); H=$(cd "$(dirname "$0")" && pwd); U=$(cd "$H/../.." && pwd); O="$U/Saved/fog-oracle"; mkdir -p "$O"
SRC="$U/Source/ThreeCrossroads/World"
node "$H/fog-oracle.mjs" "$REF" "$O/fog-oracle.json"
node "$H/to-input.mjs" "$O/fog-oracle.json" "$REF" "$O/harness-input.txt"
g++ -std=c++17 -O2 -Wall -Wextra -Wshadow -I"$SRC" "$H/harness.cpp" -o "$O/harness"
"$O/harness" "$O/harness-input.txt"
node "$H/haze-oracle.mjs" "$REF" "$O/haze-oracle.json"
# regenerate the test data (compare with git diff): World/TCFogFrontier.Oracle.inl and the rows of ThreeCrossroads.Look.Haze
node "$H/to-inl.mjs" "$O/fog-oracle.json" "$REF" "$O/TCFogFrontier.Oracle.inl"
node "$H/haze-test-gen.mjs" "$O/haze-oracle.json" "$O/haze-rows.txt"
cmp -s "$O/TCFogFrontier.Oracle.inl" "$SRC/TCFogFrontier.Oracle.inl" && echo "TCFogFrontier.Oracle.inl: unchanged" || echo "TCFogFrontier.Oracle.inl: differs from $O/TCFogFrontier.Oracle.inl"
