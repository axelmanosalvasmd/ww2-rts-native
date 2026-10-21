#!/usr/bin/env bash
# Rebuild and verify the infantry for Unreal from the pinned reference. Nothing here edits reference/ww2-rts.
#
#   bash unreal/Tools/models/infantry/run.sh            export + Node/C++ oracle
#   bash unreal/Tools/models/infantry/run.sh --unreal   also: scratch-project build, import, in-engine checks,
#                                                       offscreen render, and copy of the packages into
#                                                       unreal/Content/Models/Infantry
#
# Environment (defaults in brackets):
#   SCRATCH   working directory for dependencies, the scratch Unreal project and logs [~/.hermes/cache/scratch/infantry]
#   EVIDENCE  where reports, captures and hashes go [~/Work/ww2-native-verification/models/infantry]
#   UE_ROOT   Unreal Engine 5.8 install [~/Applications/UnrealEngine-5.8.3]
# The Unreal steps never build or open the shared project: they compile TCInfantryRig plus the check commandlets
# into a throwaway project under $SCRATCH and copy only the resulting /Game/Models/Infantry packages back.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../../../.." && pwd)"
REF="$REPO/reference/ww2-rts"
SCRATCH="${SCRATCH:-$HOME/.hermes/cache/scratch/infantry}"
EVIDENCE="${EVIDENCE:-$HOME/Work/ww2-native-verification/models/infantry}"
UE_ROOT="${UE_ROOT:-$HOME/Applications/UnrealEngine-5.8.3}"
OUT="$REPO/unreal/Content/Models/Infantry/Source"
mkdir -p "$SCRATCH" "$EVIDENCE"

# three.js exactly as the pin locks it (npm ci on copies of its package.json and package-lock.json)
DEPS="$SCRATCH/deps"
if [ ! -f "$DEPS/node_modules/three/package.json" ]; then
  mkdir -p "$DEPS"
  cp "$REF/package.json" "$REF/package-lock.json" "$DEPS/"
  (cd "$DEPS" && npm ci --ignore-scripts --no-audit --no-fund >/dev/null)
fi
export INFANTRY_DEPS="$DEPS"
NODE=(node --import "$HERE/register.mjs")
echo "three $(node -p "require('$DEPS/node_modules/three/package.json').version"), node $(node --version)"

echo "== export"
rm -rf "$OUT"
"${NODE[@]}" "$HERE/export-infantry.mjs" "$OUT" > "$EVIDENCE/export.json"

echo "== oracle"
O="$SCRATCH/oracle"; mkdir -p "$O"
"${NODE[@]}" "$HERE/oracle/dump-reference.mjs" "$O"
g++ -std=c++20 -O2 -Wall -Wextra -Wshadow -Werror -o "$SCRATCH/rig-harness" "$HERE/oracle/rig-harness.cpp"
"$SCRATCH/rig-harness" < "$O/cases.txt" > "$O/port.jsonl"
node "$HERE/oracle/compare.mjs" "$O/reference.jsonl" "$O/port.jsonl" "$EVIDENCE/oracle-joints.json"
"${NODE[@]}" "$HERE/oracle/check-skin.mjs" "$OUT/SK_Infantry.glb" "$O/reference.jsonl" "$O/port.jsonl" "$EVIDENCE/oracle-skin.json"
"${NODE[@]}" "$HERE/oracle/dump-vertices.mjs" "$SCRATCH/vertices.json"
node "$HERE/oracle/squad-reference.mjs" "$O/squad"
g++ -std=c++20 -O2 -Wall -Wextra -Wshadow -Werror -o "$SCRATCH/squad-harness" "$HERE/oracle/squad-harness.cpp"
"$SCRATCH/squad-harness" < "$O/squad/scenario.txt" > "$O/squad/port.jsonl"
node "$HERE/oracle/squad-compare.mjs" "$O/squad/squad-reference.jsonl" "$O/squad/port.jsonl" "$EVIDENCE/oracle-squad.json"

[ "${1:-}" = "--unreal" ] || exit 0

echo "== scratch Unreal project"
U="$SCRATCH/ue/RigCheck"; M="$U/Source/ThreeCrossroads"
mkdir -p "$M/Client" "$M/Check" "$U/Config"
cat > "$U/RigCheck.uproject" <<'JSON'
{
	"FileVersion": 3,
	"EngineAssociation": "UE_5.8",
	"Description": "Scratch compile and commandlet check of TCInfantryRig (ww2-rts-native models/infantry). Not a project.",
	"Modules": [ { "Name": "ThreeCrossroads", "Type": "Runtime", "LoadingPhase": "Default" } ],
	"Plugins": [ { "Name": "PythonScriptPlugin", "Enabled": true } ]
}
JSON
cat > "$U/Source/RigCheckEditor.Target.cs" <<'CS'
using UnrealBuildTool;
public class RigCheckEditorTarget : TargetRules
{
	public RigCheckEditorTarget(TargetInfo Target) : base(Target)
	{
		Type = TargetType.Editor;
		DefaultBuildSettings = BuildSettingsVersion.Latest;
		IncludeOrderVersion = EngineIncludeOrderVersion.Latest;
		ExtraModuleNames.Add("ThreeCrossroads");
	}
}
CS
cat > "$M/ThreeCrossroads.Build.cs" <<'CS'
using UnrealBuildTool;
public class ThreeCrossroads : ModuleRules
{
	public ThreeCrossroads(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;
		// the same public dependencies as the real ThreeCrossroads module
		PublicDependencyModuleNames.AddRange(new string[] { "Core", "CoreUObject", "Engine", "InputCore" });
		PublicIncludePaths.Add(ModuleDirectory);
		// the verification commandlets (unreal/Tools/models/infantry/ue-check) only
		PrivateDependencyModuleNames.AddRange(new string[] { "Json", "MeshDescription", "StaticMeshDescription", "RenderCore", "RHI" });
		CppCompileWarningSettings.ShadowVariableWarningLevel = WarningLevel.Error;
		CppCompileWarningSettings.UndefinedIdentifierWarningLevel = WarningLevel.Error;
	}
}
CS
printf '#include "Modules/ModuleManager.h"\nIMPLEMENT_PRIMARY_GAME_MODULE(FDefaultGameModuleImpl, ThreeCrossroads, "ThreeCrossroads");\n' > "$M/ThreeCrossroads.cpp"
# copies, not links: UnrealBuildTool dates the link, not its target
cp "$REPO/unreal/Source/ThreeCrossroads/Client/TCInfantryRig.h" "$REPO/unreal/Source/ThreeCrossroads/Client/TCInfantryRig.cpp" "$M/Client/"
cp "$HERE/ue-check/TCInfantryRigCheck.h" "$HERE/ue-check/TCInfantryRigCheck.cpp" "$M/Check/"
nice -n 10 "$UE_ROOT/Engine/Build/BatchFiles/Linux/Build.sh" RigCheckEditor Linux Development -Project="$U/RigCheck.uproject" -MaxParallelActions=6 > "$EVIDENCE/ue-build.log" 2>&1
grep -E "warning|error" "$EVIDENCE/ue-build.log" && { echo "build reported warnings or errors"; exit 1; } || true
CMD=(nice -n 10 "$UE_ROOT/Engine/Binaries/Linux/UnrealEditor-Cmd" "$U/RigCheck.uproject" -unattended -nop4 -nosplash)

echo "== import"
rm -rf "$U/Content/Models"
INFANTRY_EXPORT="$OUT" INFANTRY_REFERENCE="$REF" INFANTRY_LOG="$EVIDENCE/ue-import.json" \
  "${CMD[@]}" -run=pythonscript -script="$HERE/import_infantry.py" -NullRHI > "$EVIDENCE/ue-import.log" 2>&1
python3 -c "import json,sys; d=json.load(open('$EVIDENCE/ue-import.json')); sys.exit(1 if d['errors'] else 0)"

echo "== in-engine checks"
"${CMD[@]}" -run=TCInfantryRigCheck -Vertices="$SCRATCH/vertices.json" -Out="$EVIDENCE/ue-check.json" -NullRHI > "$EVIDENCE/ue-check.log" 2>&1
"${CMD[@]}" -run=TCInfantryRenderCheck -Kits="$OUT/kits.json" -Png="$EVIDENCE/render.png" -Out="$EVIDENCE/ue-render.json" \
  -RenderOffscreen -AllowCommandletRendering > "$EVIDENCE/ue-render.log" 2>&1

echo "== copy packages"
DEST="$REPO/unreal/Content/Models/Infantry"
find "$DEST" -name '*.uasset' -delete
(cd "$U/Content/Models/Infantry" && find . -name '*.uasset' -print0 | while IFS= read -r -d '' f; do mkdir -p "$DEST/$(dirname "$f")"; cp "$f" "$DEST/$f"; done)
(cd "$DEST" && find . -type f | sort | xargs sha256sum) > "$EVIDENCE/content-sha256.txt"
echo "done: $(grep -c uasset "$EVIDENCE/content-sha256.txt") packages"
