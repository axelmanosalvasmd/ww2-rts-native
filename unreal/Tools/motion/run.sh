#!/usr/bin/env bash
# Unit motion for Unreal (Client/TCUnitMotion.*, Client/TCInfantryRig.*): oracles, far-figure export, materials and
# the scratch-project check. Nothing here edits reference/ww2-rts or builds the shared project.
#
#   bash unreal/Tools/motion/run.sh            oracles (rigOf joints, skinning, squad followers, the motion scenario)
#                                              and the far-figure export into unreal/Content/Models/Motion
#   bash unreal/Tools/motion/run.sh --unreal   also: the crowd materials (make-materials.py on a content-only host
#                                              whose Content/Models is the real one), a scratch C++ project compiling
#                                              copies of TCInfantryRig, TCUnitMotion and TCModelLibrary with the check
#                                              commandlet, its automation tests and the offscreen render
#
# Environment (defaults in brackets):
#   SCRATCH   working directory [~/.hermes/cache/scratch/motion]
#   EVIDENCE  reports and captures [~/Work/ww2-native-verification/slice-11/motion]
#   UE_ROOT   Unreal Engine 5.8 [~/Applications/UnrealEngine-5.8.3]
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
REF="$REPO/reference/ww2-rts"
SCRATCH="${SCRATCH:-$HOME/.hermes/cache/scratch/motion}"
EVIDENCE="${EVIDENCE:-$HOME/Work/ww2-native-verification/slice-11/motion}"
UE_ROOT="${UE_ROOT:-$HOME/Applications/UnrealEngine-5.8.3}"
INF="$REPO/unreal/Tools/models/infantry"
CONTENT="$REPO/unreal/Content/Models/Motion"
mkdir -p "$SCRATCH" "$EVIDENCE" "$CONTENT"

# three.js exactly as the pin locks it (npm ci on copies of its package.json and package-lock.json)
DEPS="${DEPS:-$SCRATCH/deps}"
if [ ! -f "$DEPS/node_modules/three/package.json" ]; then
  mkdir -p "$DEPS"
  cp "$REF/package.json" "$REF/package-lock.json" "$DEPS/"
  (cd "$DEPS" && npm ci --ignore-scripts --no-audit --no-fund >/dev/null)
fi
export INFANTRY_DEPS="$DEPS"
echo "three $(node -p "require('$DEPS/node_modules/three/package.json').version"), node $(node --version), $(g++ --version | head -1)"
CXX=(g++ -std=c++20 -O2 -Wall -Wextra -Wshadow -Werror)

echo "== oracle: rigOf joints, skin and squad followers (unreal/Tools/models/infantry/oracle on this TCInfantryRig.cpp)"
O="$SCRATCH/rig"; mkdir -p "$O/squad"
node --no-warnings --import "$INF/register.mjs" "$INF/oracle/dump-reference.mjs" "$O"
"${CXX[@]}" -o "$SCRATCH/rig-harness" "$INF/oracle/rig-harness.cpp"
"$SCRATCH/rig-harness" < "$O/cases.txt" > "$O/port.jsonl"
node "$INF/oracle/compare.mjs" "$O/reference.jsonl" "$O/port.jsonl" "$EVIDENCE/oracle-rig-joints.json"
node --no-warnings --import "$INF/register.mjs" "$INF/oracle/check-skin.mjs" "$REPO/unreal/Content/Models/Infantry/Source/SK_Infantry.glb" "$O/reference.jsonl" "$O/port.jsonl" "$EVIDENCE/oracle-rig-skin.json"
node "$INF/oracle/squad-reference.mjs" "$O/squad"
"${CXX[@]}" -o "$SCRATCH/squad-harness" "$INF/oracle/squad-harness.cpp"
"$SCRATCH/squad-harness" < "$O/squad/scenario.txt" > "$O/squad/port.jsonl"
node "$INF/oracle/squad-compare.mjs" "$O/squad/squad-reference.jsonl" "$O/squad/port.jsonl" "$EVIDENCE/oracle-rig-squad.json"

echo "== oracle: animate(), model-motion, wheels, tracks, kaiju, planes, corpses"
M="$SCRATCH/oracle"; mkdir -p "$M"
node --no-warnings --import "$HERE/register.mjs" "$HERE/oracle/motion-reference.mjs" "$M"
"${CXX[@]}" -o "$SCRATCH/motion-harness" "$HERE/oracle/motion-harness.cpp"
"$SCRATCH/motion-harness" < "$M/scenario.txt" > "$M/port.jsonl"
node "$HERE/oracle/compare.mjs" "$M/reference.jsonl" "$M/port.jsonl" "$EVIDENCE/oracle-motion.json"

echo "== far figures (twice, for determinism)"
for RUN in a b; do node --no-warnings --import "$HERE/register.mjs" "$HERE/export-far-figures.mjs" "$SCRATCH/far-$RUN" > "$SCRATCH/far-$RUN.log"; done
cmp "$SCRATCH/far-a/FarFigures.bin" "$SCRATCH/far-b/FarFigures.bin"
cp "$SCRATCH/far-a/FarFigures.bin" "$SCRATCH/far-a/FarFigures.json" "$CONTENT/"
cp "$SCRATCH/far-a/FarFigures.json" "$EVIDENCE/far-figures.json"
cat "$SCRATCH/far-a.log"

[ "${1:-}" = "--unreal" ] || exit 0

echo "== crowd materials (content-only host, Content/Models -> the real one)"
H="$SCRATCH/MotionHost"; mkdir -p "$H/Content"
cat > "$H/MotionHost.uproject" <<'JSON'
{
	"FileVersion": 3,
	"EngineAssociation": "UE_5.8",
	"Category": "",
	"Description": "Scratch content-only host for unreal/Tools/motion/make-materials.py (Content/Models links to the real project)",
	"Plugins": [ { "Name": "PythonScriptPlugin", "Enabled": true } ]
}
JSON
ln -sfn "$REPO/unreal/Content/Models" "$H/Content/Models"
MOTION_LOG="$EVIDENCE/materials.json" nice -n 10 "$UE_ROOT/Engine/Binaries/Linux/UnrealEditor-Cmd" "$H/MotionHost.uproject" -run=pythonscript \
  -script="$HERE/make-materials.py" -unattended -nop4 -nosplash -nullrhi > "$SCRATCH/materials.log" 2>&1
python3 -c "import json,sys; d=json.load(open('$EVIDENCE/materials.json')); print(len(d['assets']), 'materials'); sys.exit(1 if d['errors'] else 0)"

echo "== scratch C++ project (copies of the client files and the check commandlet)"
U="$SCRATCH/ue/MotionCheck"; S="$U/Source/ThreeCrossroads"
mkdir -p "$S/Client" "$S/Check" "$U/Content" "$U/Config"
ln -sfn "$REPO/unreal/Content/Models" "$U/Content/Models"
cat > "$U/MotionCheck.uproject" <<'JSON'
{
	"FileVersion": 3,
	"EngineAssociation": "UE_5.8",
	"Description": "Scratch compile, tests and render of TCUnitMotion and TCInfantryRig (ww2-rts-native slice 11 motion). Not a project.",
	"Modules": [ { "Name": "ThreeCrossroads", "Type": "Runtime", "LoadingPhase": "Default" } ]
}
JSON
cat > "$U/Source/MotionCheckEditor.Target.cs" <<'CS'
using UnrealBuildTool;
public class MotionCheckEditorTarget : TargetRules
{
	public MotionCheckEditorTarget(TargetInfo Target) : base(Target)
	{
		Type = TargetType.Editor;
		DefaultBuildSettings = BuildSettingsVersion.Latest;
		IncludeOrderVersion = EngineIncludeOrderVersion.Latest;
		ExtraModuleNames.Add("ThreeCrossroads");
	}
}
CS
cat > "$S/ThreeCrossroads.Build.cs" <<'CS'
using UnrealBuildTool;
public class ThreeCrossroads : ModuleRules
{
	public ThreeCrossroads(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;
		// the real module's public dependencies and the private ones these files use (Json, MeshDescription,
		// StaticMeshDescription, RenderCore are all in the real ThreeCrossroads.Build.cs)
		PublicDependencyModuleNames.AddRange(new string[] { "Core", "CoreUObject", "Engine", "InputCore" });
		PrivateDependencyModuleNames.AddRange(new string[] { "Json", "MeshDescription", "StaticMeshDescription", "RenderCore", "RHI" });
		PublicIncludePaths.Add(ModuleDirectory);
		CppCompileWarningSettings.ShadowVariableWarningLevel = WarningLevel.Error;
		CppCompileWarningSettings.UndefinedIdentifierWarningLevel = WarningLevel.Error;
	}
}
CS
printf '#include "Modules/ModuleManager.h"\nIMPLEMENT_PRIMARY_GAME_MODULE(FDefaultGameModuleImpl, ThreeCrossroads, "ThreeCrossroads");\n' > "$S/ThreeCrossroads.cpp"
for F in TCInfantryRig TCUnitMotion TCModelLibrary; do cp "$REPO/unreal/Source/ThreeCrossroads/Client/$F.h" "$REPO/unreal/Source/ThreeCrossroads/Client/$F.cpp" "$S/Client/"; done
cp "$HERE/ue-check/TCMotionCheck.h" "$HERE/ue-check/TCMotionCheck.cpp" "$S/Check/"
nice -n 10 "$UE_ROOT/Engine/Build/BatchFiles/Linux/Build.sh" MotionCheckEditor Linux Development -Project="$U/MotionCheck.uproject" -MaxParallelActions=6 > "$EVIDENCE/ue-build.log" 2>&1
grep -E "warning|error" "$EVIDENCE/ue-build.log" && { echo "build reported warnings or errors"; exit 1; } || true
CMD=(nice -n 10 "$UE_ROOT/Engine/Binaries/Linux/UnrealEditor-Cmd" "$U/MotionCheck.uproject" -unattended -nop4 -nosplash)

echo "== automation tests (ThreeCrossroads.Motion and ThreeCrossroads.Models.Library)"
# the editor force-exits before its stdout is flushed: read the exported report and the project's own log instead
rm -rf "$EVIDENCE/tests"
"${CMD[@]}" -nullrhi -NoSound -ExecCmds="Automation RunTests ThreeCrossroads; Quit" -ReportExportPath="$EVIDENCE/tests" > /dev/null 2>&1 || true
cp "$U/Saved/Logs/MotionCheck.log" "$EVIDENCE/ue-tests.log"
python3 - "$EVIDENCE/tests/index.json" <<'PY' | tee "$EVIDENCE/ue-tests-summary.txt"
import json, sys
d = json.load(open(sys.argv[1], encoding='utf-8-sig'))
for t in d['tests']:
    notes = [e['event']['message'] for e in t['entries'] if e['event']['type'] in ('Info', 'Warning', 'Error')]
    print(t['fullTestPath'], t['state'], '; '.join(notes))
print('succeeded', d['succeeded'], 'failed', d['failed'], 'not run', d['notRun'])
sys.exit(0 if d['failed'] == 0 and d['notRun'] == 0 and d['succeeded'] > 0 else 1)
PY

echo "== render check"
"${CMD[@]}" -run=TCMotionCheck -Out="$EVIDENCE/ue-check.json" -Png="$EVIDENCE/render.png" -RenderOffscreen -AllowCommandletRendering > "$EVIDENCE/ue-check.log" 2>&1
python3 -c "import json,sys; d=json.load(open('$EVIDENCE/ue-check.json')); print('check', 'pass' if d['pass'] else d['failures']); sys.exit(0 if d['pass'] else 1)"
(cd "$CONTENT" && find . -type f | sort | xargs sha256sum) > "$EVIDENCE/content-sha256.txt"
