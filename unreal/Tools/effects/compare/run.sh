#!/usr/bin/env bash
# Effects side by side: the pinned client/fx.js in headless Chromium (fx-compare.mjs), then the Unreal client's
# ATCEffects (-run=TCEffectsShot), both playing scenes.json from the same camera and light; pairs land in <out>/pairs.
#
#   unreal/Tools/effects/compare/run.sh <out dir> <three deps dir> [project.uproject]
#   env: UE (engine root)
# Needs a built ThreeCrossroadsEditor, imported Content/Effects (unreal/Tools/effects/run.sh) and Content/Models.
# Nothing here edits reference/ww2-rts.
set -euo pipefail
OUT=$1 DEPS=$2
HERE=$(cd "$(dirname "$0")" && pwd); REPO=$(cd "$HERE/../../../.." && pwd)
PROJECT=${3:-$REPO/unreal/ThreeCrossroads.uproject}
UE=${UE:-$HOME/Applications/UnrealEngine-5.8.3}
mkdir -p "$OUT/pairs"
node "$HERE/shoot-fx-reference.mjs" --three "$DEPS" --out "$OUT" --profile "$OUT/.chromium-profile"
"$UE/Engine/Binaries/Linux/UnrealEditor-Cmd" "$PROJECT" -run=TCEffectsShot -Scenes="$HERE/scenes.json" -Out="$OUT" \
  -AllowCommandletRendering -RenderOffscreen -unattended -nosplash -nop4 -NoSound -abslog="$OUT/unreal.log" > /dev/null 2>&1
grep -E "effects shot" "$OUT/unreal.log" | sed 's/^\[[^]]*\]\[[^]]*\]//'
for ref in "$OUT"/*-reference.png; do
  base=$(basename "$ref" -reference.png); un="$OUT/$base-unreal.png"
  [ -f "$un" ] || { echo "missing $un" >&2; continue; }
  magick "$ref" "$un" +append -resize 1600x \( -size 1600x36 xc:'#202020' -fill white -pointsize 20 \
    -annotate +12+25 "$base   left: reference three.js client/fx.js (e4e579a)   right: Unreal ATCEffects" \) -swap 0,1 -append "$OUT/pairs/$base.png"
done
echo "pairs: $(ls "$OUT/pairs" | wc -l) in $OUT/pairs"
