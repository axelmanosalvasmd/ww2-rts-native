#!/usr/bin/env bash
# Round 17 (hudui): the reference's typeface for the native HUD. reference/ww2-rts/client/index.html links Google Fonts'
# css2 for 'Barlow Semi Condensed' (weights 400, 500, 600, 700) and sets every word in it. This fetches that same
# stylesheet (the URL is read from index.html, not retyped), downloads the four TrueType files it names (the files
# Chromium downloads), checks them against the SHA-256 sums below, and writes them with the SIL Open Font License to
# unreal/Content/Fonts/BarlowSemiCondensed. Client/TCStats.h FTCType reads them (widths and drawing); without them the
# HUD keeps its own font. Nothing in reference/ changes.
#   usage: unreal/Tools/fonts/fetch-barlow.sh [--check]     (--check: only verify the files already there)
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); UNREAL=$(cd "$HERE/../.." && pwd); REPO=$(cd "$UNREAL/.." && pwd)
OUT="$UNREAL/Content/Fonts/BarlowSemiCondensed"
declare -A SUM=(
  [Regular]=9ff9499736c5fad7db00815aafc939b3df67bdee87ce62fb04257fe167c151fd
  [Medium]=c2b046235fb4c2f81ca58bbdca2313e9681758295b129931853324ade56180c9
  [SemiBold]=cb75439ae401f77624d5b268ea25fd7556e34803fccbd2f92e937a4f3856a1da
  [Bold]=6098b66ce7d608dcea8c6a2eeacfdeeee5416e0e0c12a2318595d3dd8c5babef
  [OFL]=10b0e6bb43166a50fbe7cde2895bd7f703738a5da843acb5c97c95dc9ea26817
)
declare -A WEIGHT=([400]=Regular [500]=Medium [600]=SemiBold [700]=Bold)
check() { local f=$1 want=$2; [[ -f $f ]] && [[ $(sha256sum "$f" | cut -d' ' -f1) == "$want" ]]; }
verify() {
  local bad=0
  for n in Regular Medium SemiBold Bold; do check "$OUT/BarlowSemiCondensed-$n.ttf" "${SUM[$n]}" || { echo "fonts: BarlowSemiCondensed-$n.ttf missing or changed" >&2; bad=1; }; done
  check "$OUT/OFL.txt" "${SUM[OFL]}" || { echo "fonts: OFL.txt missing or changed" >&2; bad=1; }
  return $bad
}
if [[ ${1:-} == --check ]]; then verify && echo "fonts: the four Barlow Semi Condensed files and OFL.txt match"; exit; fi
CSS_URL=$(grep -o 'https://fonts.googleapis.com/css2?family=Barlow+Semi+Condensed[^"]*' "$REPO/reference/ww2-rts/client/index.html" | head -1 | sed 's/&amp;/\&/g')
[[ -n $CSS_URL ]] || { echo "fonts: no Barlow Semi Condensed link in reference/ww2-rts/client/index.html" >&2; exit 1; }
mkdir -p "$OUT"; TMP=$(mktemp -d "$OUT/.fetch.XXXXXX"); trap 'rm -rf "$TMP"' EXIT
# curl's own user agent gets the stylesheet with whole .ttf files (a browser's gets woff2 subsets of the same faces)
curl -fsS "$CSS_URL" -o "$TMP/fonts.css"
echo "fonts: $CSS_URL"
for w in 400 500 600 700; do
  n=${WEIGHT[$w]}
  url=$(awk -v W="$w" '/font-weight/ { ok = ($2 == W ";") } ok && /src:/ { match($0, /https:[^)]*/); print substr($0, RSTART, RLENGTH); ok = 0 }' "$TMP/fonts.css")
  [[ -n $url ]] || { echo "fonts: weight $w not in the stylesheet" >&2; exit 1; }
  curl -fsS "$url" -o "$TMP/BarlowSemiCondensed-$n.ttf"
  check "$TMP/BarlowSemiCondensed-$n.ttf" "${SUM[$n]}" || { echo "fonts: $url does not match the pinned sum (the font changed upstream; check it and update SUM)" >&2; exit 1; }
  echo "fonts: $w $url"
done
# the licence the family ships with (google/fonts ofl/barlowsemicondensed)
# (one line ends in a space upstream; trailing blanks are dropped so git diff --check stays clean: the words are unchanged)
curl -fsS https://raw.githubusercontent.com/google/fonts/main/ofl/barlowsemicondensed/OFL.txt | sed 's/[ \t]*$//' > "$TMP/OFL.txt"
check "$TMP/OFL.txt" "${SUM[OFL]}" || { echo "fonts: OFL.txt does not match the pinned sum" >&2; exit 1; }
mv "$TMP"/BarlowSemiCondensed-*.ttf "$TMP/OFL.txt" "$OUT/"
verify && echo "fonts: written to $OUT"
