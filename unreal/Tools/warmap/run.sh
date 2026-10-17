#!/usr/bin/env bash
# Regenerates Content/WarMap from the pinned reference (see README.md). Usage:
#   unreal/Tools/warmap/run.sh <scratch work dir> [project file]
# Needs node, Chromium (CHROMIUM, default /usr/bin/chromium), fontconfig's fc-match and a built editor target.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$1"; PROJECT="${2:-$HERE/../../ThreeCrossroads.uproject}"
UE="${UE:-$HOME/Applications/UnrealEngine-5.8.3}"
CMD="$UE/Engine/Binaries/Linux/UnrealEditor-Cmd"
mkdir -p "$WORK"
node "$HERE/make-warmap.mjs" assets --work "$WORK" --chromium "${CHROMIUM:-/usr/bin/chromium}"
FONT="$WORK/font/$(node -e "console.log(require('$WORK/WarMapData.json').font.file)")"
"$CMD" "$PROJECT" -run=TCWarMapPaint -ImportFont="$FONT" -unattended -nop4 -nosplash -nullrhi > "$WORK/font-import.log" 2>&1
"$CMD" "$PROJECT" -run=pythonscript -EnablePlugins=PythonScriptPlugin -unattended -nop4 -nosplash -nullrhi \
  -script="$HERE/import-warmap.py --work $WORK" > "$WORK/import.log" 2>&1
grep -h "TCWarMapPaint: font face\|\[warmap\]" "$WORK/font-import.log" "$WORK/import.log"
