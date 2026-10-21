#!/usr/bin/env bash
# The light and color comparison, end to end (README.md). Steps can be run one at a time:
#   run.sh shoot   <out> <three deps> <set> <assets>   the reference half of one set into <out>/<set>
#   run.sh assets  <out>/<set> <three deps> [--skip-masters]   the look assets (editor Python, a content host project)
#   run.sh unreal  <out> <light> <set>...              -run=TCLookCompare -Light=<light> for each set
#   run.sh metrics <out> <json> <set>...               metrics.mjs over the sets
#   run.sh pairs   <out> <set>...                      reference | before | game before | look, one picture per view
# env: UE (engine), PROJECT (the .uproject with TCLookCompare built), HOST (a content-only project with
# PythonScriptPlugin whose Content/Models links to unreal/Content/Models)
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); REPO=$(cd "$HERE/../../.." && pwd)
UE=${UE:-$HOME/Applications/UnrealEngine-5.8.3}; PROJECT=${PROJECT:-$REPO/unreal/ThreeCrossroads.uproject}
cmd=$1; shift
case $cmd in
  shoot) node "$HERE/shoot.mjs" --three "$2" --out "$1/$3" --profile "$1/$3/.chromium-profile" --assets "$4" ;;
  assets) "$UE/Engine/Binaries/Linux/UnrealEditor-Cmd" "${HOST:?HOST: a content host project}" -run=pythonscript -unattended -nop4 -nosplash -nullrhi \
            -script="$HERE/make-look-assets.py --look $1 --three $2 --work $1/.work ${3:-}" ;;
  unreal) out=$1; light=$2; shift 2
    for s in "$@"; do
      "$UE/Engine/Binaries/Linux/UnrealEditor-Cmd" "$PROJECT" -run=TCLookCompare -Cameras="$out/$s/cameras.json" -Out="$out/$s" -Light="$light" \
        -AllowCommandletRendering -RenderOffscreen -unattended -nosplash -nop4 -NoSound -abslog="$out/$s/unreal-$light.log" > /dev/null 2>&1 || true
      grep -o "LOOK $light [A-Z]*: [0-9]* failures" "$out/$s/unreal-$light.log" || echo "$s: no result line"
    done ;;
  metrics) out=$1; json=$2; shift 2; node "$HERE/metrics.mjs" $(for s in "$@"; do echo --dir "$out/$s"; done) --lights before,gamebefore,look --out "$json" ;;
  pairs) out=$1; shift
    for s in "$@"; do
      mkdir -p "$out/$s/pairs"
      for ref in "$out/$s"/*-reference.png; do
        base=$(basename "$ref" -reference.png)
        case $base in *-mask|*-plate) continue ;; esac
        pics=("$ref"); labels="reference three.js r186"
        for l in before gamebefore look; do [ -f "$out/$s/$base-unreal-$l.png" ] && { pics+=("$out/$s/$base-unreal-$l.png"); labels="$labels | Unreal $l"; }; done
        magick "${pics[@]}" -resize 640x +append \( -size "$((640 * ${#pics[@]}))x30" xc:'#202020' -fill white -pointsize 18 -annotate +10+21 "$base    $labels" \) -swap 0,1 -append "$out/$s/pairs/$base.png"
      done
      echo "$s: $(ls "$out/$s/pairs" | wc -l) pictures"
    done ;;
  *) echo "unknown step $cmd" >&2; exit 2 ;;
esac
