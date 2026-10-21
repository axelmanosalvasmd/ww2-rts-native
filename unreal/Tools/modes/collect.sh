#!/usr/bin/env bash
# Joins the rows of chosen runs into one table and copies their captures as modes-<mode id>-<what>.png.
#   collect.sh <screenshots dir> <table.md> <run dir> [<run dir> ...]
# A Tutorial step capture tutorial-<lang>-step-N-of-9.png becomes modes-<id>-step-N-of-9-<lang>.png; a guest capture
# keeps its name (the run gives the guest's captures an -es suffix); a tour pose keeps its name (tours name them
# modes-...). When one mode id has more than one run in the list, each copy also carries the run's suffix (the run dir's
# name after "<id>-"), so nothing is overwritten.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SHOTS="$1"; TABLE="$2"; shift 2
mkdir -p "$SHOTS"
node "$HERE/summarize.mjs" --table "$@" > "$TABLE"
idof() { sed -n 's/^mode id: \([^;]*\);.*/\1/p' "$1/cmdlines.txt" 2>/dev/null | head -1; }
declare -A COUNT
for d in "$@"; do i=$(idof "$d"); COUNT[$i]=$(( ${COUNT[$i]:-0} + 1 )); done
for d in "$@"; do
  id=$(idof "$d"); [ -n "$id" ] || id=$(basename "$d")
  b0=$(basename "$d"); suf=""; [ "${COUNT[$id]}" -gt 1 ] && suf="-${b0#"$id"-}"
  for f in "$d"/host/*.png "$d"/guest/*.png; do
    [ -f "$f" ] || continue
    b=$(basename "$f" .png)
    case "$b" in
      tutorial-*) lang=$(echo "$b" | cut -d- -f2); n="modes-$id-${b#tutorial-$lang-}-$lang" ;;
      modes-*) n="$b" ;;
      *) n="modes-$id-$b" ;;
    esac
    cp "$f" "$SHOTS/$n$suf.png"
  done
done
cat "$TABLE"
