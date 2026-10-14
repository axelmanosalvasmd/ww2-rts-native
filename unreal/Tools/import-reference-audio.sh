#!/usr/bin/env bash
# Imports the pinned reference game's sound effects and voice lines into unreal/Content/Audio, unchanged (parity gap 9,
# CLI-115 to CLI-119; owner decision: reuse the reference files as they are, the UK has no voices).
#
#   unreal/Tools/import-reference-audio.sh          import, then write unreal/Content/Audio/import-manifest.json
#   unreal/Tools/import-reference-audio.sh check    only compare the manifest with the reference (no editor run)
#
# The import runs UnrealEditor-Cmd -run=pythonscript (import-reference-audio.py) inside a throwaway project of its own
# under the scratch folder, with the Python and editor scripting plugins enabled there only. The game project, its
# config, its Binaries and the shared editor build are not touched; only Content/Audio/SFX, Content/Audio/Voice and the
# manifest are replaced at the end.
#
# Environment (all optional):
#   UE            engine folder (default ~/Applications/UnrealEngine-5.8.3)
#   TC_SCRATCH    work folder (default ~/.hermes/cache/scratch/tc-audio-import)
#   TC_AUDIO_PIN  the reference commit to require (default e4e579a838aabae3b1d9dadbc17a0a87147bc52d, the repo's pin)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$HERE/.." && pwd)"
REPO="$(cd "$PROJECT_DIR/.." && pwd)"
REF="$REPO/reference/ww2-rts"
UE="${UE:-$HOME/Applications/UnrealEngine-5.8.3}"
WORK="${TC_SCRATCH:-$HOME/.hermes/cache/scratch/tc-audio-import}"
PIN="${TC_AUDIO_PIN:-e4e579a838aabae3b1d9dadbc17a0a87147bc52d}"
DEST="$PROJECT_DIR/Content/Audio"
MANIFEST="$DEST/import-manifest.json"

die() { echo "import-reference-audio: $*" >&2; exit 1; }

# the reference must be the pinned commit with client/audio exactly as committed
head="$(git -C "$REF" rev-parse HEAD 2>/dev/null)" || die "no git checkout at $REF"
[[ "$head" == "$PIN" ]] || die "reference is at $head, expected the pin $PIN"
[[ -z "$(git -C "$REF" status --porcelain -- client/audio)" ]] || die "reference client/audio has local changes"

check() {
	[[ -f "$MANIFEST" ]] || die "no manifest at $MANIFEST: run the import first"
	command -v jq >/dev/null || die "jq is needed for check"
	[[ "$(jq -r .pin "$MANIFEST")" == "$PIN" ]] || die "manifest is for another pin"
	local bad=0 n=0
	while IFS=$'\t' read -r src sha asset; do
		n=$((n + 1))
		[[ "$(sha256sum "$REF/$src" | cut -d' ' -f1)" == "$sha" ]] || { echo "changed: $src"; bad=1; }
		[[ -f "$PROJECT_DIR/$asset" ]] || { echo "missing asset: $asset"; bad=1; }
	done < <(jq -r '.files[] | [.source, .sha256, .assetFile] | @tsv' "$MANIFEST")
	[[ "$(sha256sum "$REF/client/audio/index.json" | cut -d' ' -f1)" == "$(jq -r .index.sha256 "$MANIFEST")" ]] || { echo "changed: client/audio/index.json"; bad=1; }
	((bad == 0)) || die "manifest and reference disagree"
	echo "import-reference-audio: $n files match the reference at $PIN"
}

if [[ "${1:-}" == "check" ]]; then check; exit 0; fi
[[ $# -eq 0 ]] || die "unknown argument $1"

EDITOR="$UE/Engine/Binaries/Linux/UnrealEditor-Cmd"
[[ -x "$EDITOR" ]] || die "no editor at $EDITOR"

# a fresh throwaway project: no modules, only the plugins the script needs
rm -rf "$WORK"
mkdir -p "$WORK/project/Content" "$WORK/project/Config"
cat > "$WORK/project/AudioImport.uproject" <<'JSON'
{
	"FileVersion": 3,
	"EngineAssociation": "UE_5.8",
	"Description": "Throwaway project for unreal/Tools/import-reference-audio.sh",
	"Plugins": [
		{ "Name": "PythonScriptPlugin", "Enabled": true },
		{ "Name": "EditorScriptingUtilities", "Enabled": true }
	]
}
JSON
git -C "$REF" ls-files -s -- client/audio | awk '{ print $2 " " $4 }' > "$WORK/blobs.txt"

echo "import-reference-audio: importing from $REF/client/audio (pin $PIN) in $WORK/project"
TC_AUDIO_REF="$REF" TC_AUDIO_PIN="$PIN" TC_AUDIO_BLOBS="$WORK/blobs.txt" TC_AUDIO_OUT="$WORK/import-manifest.json" \
	"$EDITOR" "$WORK/project/AudioImport.uproject" -run=pythonscript -script="$HERE/import-reference-audio.py" \
	-unattended -nullrhi -nosplash -nosound -nop4 -nosourcecontrol -stdout -FullStdOutLogOutput > "$WORK/editor.log" 2>&1 \
	|| { tail -40 "$WORK/editor.log" >&2; die "the editor run failed (log: $WORK/editor.log)"; }
[[ -f "$WORK/import-manifest.json" ]] || { grep -i "error" "$WORK/editor.log" | tail -20 >&2; die "no manifest written (log: $WORK/editor.log)"; }

# replace only what this tool owns
mkdir -p "$DEST"
rm -rf "$DEST/SFX" "$DEST/Voice"
cp -r "$WORK/project/Content/Audio/SFX" "$WORK/project/Content/Audio/Voice" "$DEST/"
cp "$WORK/import-manifest.json" "$MANIFEST"
echo "import-reference-audio: $(jq -r '.counts.total' "$MANIFEST") assets in $DEST, manifest $MANIFEST"
check
