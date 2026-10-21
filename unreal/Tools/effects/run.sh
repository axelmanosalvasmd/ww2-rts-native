#!/usr/bin/env bash
# Rebuilds Content/Effects from the pinned reference: converts the textures (convert-fx-textures.mjs), then imports them
# and builds the materials in a content-only host project (import-effects.py), then verifies the result.
#   unreal/Tools/effects/run.sh [host-dir] [log-dir]
# host-dir (default $HOME/.hermes/cache/scratch/fxl10n/TCFxImport) is made on first use: a .uproject with the Python
# plugin and Content/Effects linked to this repo's unreal/Content/Effects. Needs node, ImageMagick (magick) and
# UE (default ~/Applications/UnrealEngine-5.8.3, override with UE=...).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
UE=${UE:-$HOME/Applications/UnrealEngine-5.8.3}
host=${1:-$HOME/.hermes/cache/scratch/fxl10n/TCFxImport}
logs=${2:-$host/logs}
mkdir -p "$host/Content" "$host/Config" "$logs" "$repo/unreal/Content/Effects"
if [ ! -f "$host/TCFxImport.uproject" ]; then
  cat > "$host/TCFxImport.uproject" <<'JSON'
{
	"FileVersion": 3,
	"EngineAssociation": "UE_5.8",
	"Category": "",
	"Description": "Scratch content-only host for unreal/Tools/effects/import-effects.py (Content/Effects links to the real project)",
	"Plugins": [ { "Name": "PythonScriptPlugin", "Enabled": true } ]
}
JSON
  : > "$host/Config/DefaultEngine.ini"
fi
[ -L "$host/Content/Effects" ] || ln -s "$repo/unreal/Content/Effects" "$host/Content/Effects"
node "$here/convert-fx-textures.mjs" "$repo/reference/ww2-rts"
export_dir="$repo/unreal/Saved/EffectsExport"
cmd="$UE/Engine/Binaries/Linux/UnrealEditor-Cmd"
"$cmd" "$host/TCFxImport.uproject" -run=pythonscript -unattended -nop4 -nosplash -nullrhi \
  -script="$here/import-effects.py --export $export_dir" > "$logs/import.log" 2>&1
"$cmd" "$host/TCFxImport.uproject" -run=pythonscript -unattended -nop4 -nosplash -nullrhi \
  -script="$here/import-effects.py --verify $logs/verify.json" > "$logs/verify.log" 2>&1
grep -E 'import-effects|Error|error:' "$logs/import.log" "$logs/verify.log" | grep -v 'LogInit\|Display: Error' || true
cat "$logs/verify.json"
