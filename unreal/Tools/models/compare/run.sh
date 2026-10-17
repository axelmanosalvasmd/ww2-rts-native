#!/usr/bin/env bash
# Side-by-side model pictures: the reference builders in headless Chromium, then the same units through the Unreal
# client's unit renderer, from the same cameras. Pairs land in <out>/pairs.
#
#   unreal/Tools/models/compare/run.sh <out dir> <three deps dir> [assets]
#     assets: type:faction:ownerSlot,... (default: the pilot, medium:0:0,at:0:0,rifle:0:0,barracks:0:0)
#   env: UE (engine root), TC_MODELS (the -TCModels= batches for the Unreal half; default all)
# Needs a built ThreeCrossroadsEditor and imported Content/Models. Nothing here edits reference/ww2-rts.
set -euo pipefail
OUT=$1 DEPS=$2 ASSETS=${3:-medium:0:0,at:0:0,rifle:0:0,barracks:0:0}
HERE=$(cd "$(dirname "$0")" && pwd); REPO=$(cd "$HERE/../../../.." && pwd)
UE=${UE:-$HOME/Applications/UnrealEngine-5.8.3}
mkdir -p "$OUT/pairs"
node "$HERE/shoot-reference.mjs" --three "$DEPS" --out "$OUT" --profile "$OUT/.chromium-profile" --assets "$ASSETS"
"$UE/Engine/Binaries/Linux/UnrealEditor-Cmd" "$REPO/unreal/ThreeCrossroads.uproject" -run=TCModelCompare -Cameras="$OUT/cameras.json" -Out="$OUT" \
  -AllowCommandletRendering -RenderOffscreen -unattended -nosplash -nop4 -NoSound ${TC_MODELS:+-TCModels=$TC_MODELS} -abslog="$OUT/unreal.log" > /dev/null 2>&1
grep -E "COMPARE" "$OUT/unreal.log" | sed 's/^\[[^]]*\]\[[^]]*\]//'
for ref in "$OUT"/*-reference.png; do
  base=$(basename "$ref" -reference.png); un="$OUT/$base-unreal.png"
  [ -f "$un" ] || { echo "missing $un" >&2; continue; }
  magick "$ref" "$un" +append -resize 1600x \( -size 1600x36 xc:'#202020' -fill white -pointsize 20 \
    -annotate +12+25 "$base   left: reference three.js (e4e579a builders)   right: Unreal ATCUnitRenderer" \) -swap 0,1 -append "$OUT/pairs/$base.png"
done
echo "pairs: $(ls "$OUT/pairs" | wc -l) in $OUT/pairs"
