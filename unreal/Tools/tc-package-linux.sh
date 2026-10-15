#!/usr/bin/env bash
# Packages the Three Crossroads client for Linux x86_64 (Development) with the installed Unreal 5.8.3 and RunUAT
# BuildCookRun, with the Rust core (ww2-ffi) linked, and checks the result.
#
#   unreal/Tools/tc-package-linux.sh check    prerequisites only: engine, toolchain, disk, ww2-ffi, generated tables
#   unreal/Tools/tc-package-linux.sh build    check, copy the project to a clean work folder, BuildCookRun, verify, and
#                                             install the package at $TC_PACKAGE_OUT (replaced only on success)
#   unreal/Tools/tc-package-linux.sh verify [DIR]   the static checks on an existing package (default $TC_PACKAGE_OUT)
#   unreal/Tools/tc-package-linux.sh stage-fixtures [DIR]   copy the Rust view's self-check stream into an existing
#                                             package (default $TC_PACKAGE_OUT; build does this itself), then verify
#
# The package needs a running game server: today the pinned JavaScript reference server (tc-local.sh serve), which is
# not part of it. Start it with the package's tc-client.sh (see the README there and unreal/README).
#
# Environment (all optional):
#   UE                       engine folder (default ~/Applications/UnrealEngine-5.8.3)
#   TC_PACKAGE_OUT           where the finished package goes (default unreal/Saved/Packages/ww2-rts-native-linux)
#   TC_PACKAGE_WORK          work folder: the project copy, its build output, the cook and the logs
#                            (default unreal/Saved/PackageWork). Kept between runs, so a second build is incremental.
#   TC_PACKAGE_ARCHIVE       UAT archive folder (default <work>/archive); a build-output folder keeps scratch smaller
#   TC_PACKAGE_CLEAN=1       empty the work folder's project copy first (a build from nothing)
#   TC_PACKAGE_JOBS          compile processes at once (default: half the logical cores, at most 8)
#   TC_PACKAGE_MIN_FREE_GB   free space needed in the work and output folders (default 30)
#   TC_PACKAGE_DEBUGINFO=1   keep the separate debug files (.debug, .sym; large) in the package
#   TC_PACKAGE_SKIP_TABLE_CHECK=1   skip the generated-table comparison with the reference (it needs node)
#
# The project is copied (not built in place) so the editor build in unreal/Binaries and unreal/Intermediate is never
# touched, and the package comes only from the files listed below plus the installed ww2-ffi.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$HERE/.." && pwd)"
REPO="$(cd "$PROJECT_DIR/.." && pwd)"
REF="$REPO/reference/ww2-rts"
UE="${UE:-$HOME/Applications/UnrealEngine-5.8.3}"
OUT="${TC_PACKAGE_OUT:-$PROJECT_DIR/Saved/Packages/ww2-rts-native-linux}"
WORK="${TC_PACKAGE_WORK:-$PROJECT_DIR/Saved/PackageWork}"
MIN_FREE_GB="${TC_PACKAGE_MIN_FREE_GB:-30}"
NPROC="$(nproc 2>/dev/null || echo 2)"
DEFAULT_JOBS=$(( NPROC / 2 )); (( DEFAULT_JOBS >= 1 )) || DEFAULT_JOBS=1; (( DEFAULT_JOBS <= 8 )) || DEFAULT_JOBS=8
JOBS="${TC_PACKAGE_JOBS:-$DEFAULT_JOBS}"
PROJECT_NAME=ThreeCrossroads
TRIPLE=x86_64-unknown-linux-gnu
FFI_DIR="$PROJECT_DIR/Plugins/WW2Core/ThirdParty/ww2_ffi/Linux/$TRIPLE"
MIRROR="$WORK/project"
# The recorded protocol stream the Rust view's startup self-check replays (WW2Core FWW2ViewReplay::SelfCheckStream) and
# where the packaged game looks for it: the plugin's folder below the package's project folder (IPluginManager's
# base dir), the same relative path as in the source tree. Only this stream is staged (about 1.4 MB); the other three
# are for the automation tests. Not cooked: loose files read with FFileHelper.
SELFCHECK_STREAM=protocol-conquest-p0
FIXTURE_REL="Plugins/WW2Core/ThirdParty/ww2_ffi/Linux/$TRIPLE/fixtures/protocol"
RUNUAT="$UE/Engine/Build/BatchFiles/RunUAT.sh"

die() { echo "tc-package: error: $*" >&2; exit 1; }
say() { echo "tc-package: $*"; }
warn() { echo "tc-package: warning: $*" >&2; }

[[ "$JOBS" =~ ^[1-9][0-9]*$ ]] || die "TC_PACKAGE_JOBS must be a positive number, got '$JOBS'"
[[ "$MIN_FREE_GB" =~ ^[0-9]+$ ]] || die "TC_PACKAGE_MIN_FREE_GB must be a number"

free_gb() { # free space in GB of the filesystem holding $1 (or its nearest existing parent)
	local d="$1"; while [[ ! -e "$d" ]]; do d="$(dirname "$d")"; done
	df -Pk "$d" | awk 'NR == 2 { printf "%d", $4 / 1048576 }'
}

toolchain() { # the clang UnrealBuildTool will use: LINUX_MULTIARCH_ROOT, else the engine's bundled SDK
	local root=""
	if [[ -n "${LINUX_MULTIARCH_ROOT:-}" ]]; then root="$LINUX_MULTIARCH_ROOT/$TRIPLE"
	else root="$(ls -d "$UE"/Engine/Extras/ThirdPartyNotUE/SDKs/HostLinux/Linux_x64/*/"$TRIPLE" 2>/dev/null | sort | tail -n 1 || true)"; fi
	[[ -n "$root" && -x "$root/bin/clang++" ]] || return 1
	echo "$root"
}

preflight() {
	say "checking prerequisites"
	[[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]] || die "packaging runs on Linux x86_64 only (Win64 is built on Windows, Mac on a Mac)"
	[[ -d "$UE/Engine" ]] || die "no Unreal Engine at $UE (set UE to the engine folder)"
	[[ -x "$RUNUAT" ]] || die "no RunUAT.sh at $RUNUAT"
	local ver; ver="$(grep -o '"MajorVersion": *[0-9]*\|"MinorVersion": *[0-9]*\|"PatchVersion": *[0-9]*' "$UE/Engine/Build/Build.version" | grep -o '[0-9]*$' | paste -sd.)"
	[[ "$ver" == 5.8.* ]] || die "the engine at $UE is $ver, the project needs 5.8"
	say "engine $ver at $UE$([[ -f "$UE/Engine/Build/InstalledBuild.txt" ]] && echo ' (installed build)')"
	if [[ -f "$UE/Engine/Build/InstalledBuild.txt" ]]; then
		[[ -d "$UE/Engine/Intermediate/Build/Linux/x64/UnrealGame/Development" ]] \
			|| die "this installed engine has no Linux game target (Engine/Intermediate/Build/Linux/x64/UnrealGame/Development is missing): add the Linux target platform to the install"
	fi
	[[ -x "$UE/Engine/Binaries/ThirdParty/DotNet/10.0/linux-x64/dotnet" ]] || ls "$UE"/Engine/Binaries/ThirdParty/DotNet/*/linux-x64/dotnet >/dev/null 2>&1 \
		|| die "the engine's bundled .NET (Engine/Binaries/ThirdParty/DotNet/*/linux-x64/dotnet) is missing: RunUAT cannot start"
	local tc
	if [[ -n "${LINUX_MULTIARCH_ROOT:-}" ]]; then
		tc="$(toolchain)" || die "LINUX_MULTIARCH_ROOT=$LINUX_MULTIARCH_ROOT has no $TRIPLE/bin/clang++ (unset it to use the engine's bundled toolchain)"
	else
		tc="$(toolchain)" || die "no Linux clang toolchain: the engine's bundled SDK (Engine/Extras/ThirdPartyNotUE/SDKs/HostLinux/Linux_x64/*/$TRIPLE) is missing; install it with the engine or set LINUX_MULTIARCH_ROOT"
	fi
	say "toolchain $("$tc/bin/clang++" --version | head -n 1) at $tc"
	command -v rsync >/dev/null || die "rsync is needed to copy the project to the work folder"
	local f
	for f in "$WORK" "$OUT"; do
		local gb; gb="$(free_gb "$f")"
		(( gb >= MIN_FREE_GB )) || die "only $gb GB free for $f, need $MIN_FREE_GB (TC_PACKAGE_MIN_FREE_GB)"
	done
	say "disk: $(free_gb "$WORK") GB free for the work folder, $(free_gb "$OUT") GB for the output"

	# the Rust core: required in a package (no interim-port fallback), and it must still match its STAMP
	[[ -f "$FFI_DIR/lib/libww2_ffi.a" && -f "$FFI_DIR/STAMP" ]] || die "ww2-ffi is not installed in $FFI_DIR: run unreal/Tools/ww2-ffi.sh build first (a package always links the Rust core)"
	"$HERE/ww2-ffi.sh" status >/dev/null 2>&1 || die "the ww2-ffi install no longer matches its STAMP: run unreal/Tools/ww2-ffi.sh build (details: unreal/Tools/ww2-ffi.sh status)"
	say "ww2-ffi: native $(sed -n 's/^native_rev=//p' "$FFI_DIR/STAMP" | cut -c1-12), rustc $(sed -n 's/^rustc=//p' "$FFI_DIR/STAMP"), matches its STAMP"

	# the generated tables: units.json and the compiled-in tables must name the pinned reference
	local units="$PROJECT_DIR/Content/Data/units.json"
	[[ -s "$units" ]] || die "$units is missing (node unreal/Tools/export-unit-table.mjs)"
	local uref; uref="$(sed -n 's/^ *"reference": *"\([0-9a-f]*\)".*/\1/p' "$units" | head -n 1)"
	[[ -n "$uref" ]] || die "$units names no reference commit"
	if [[ -e "$REF/.git" ]]; then
		local pin; pin="$(git -C "$REF" rev-parse HEAD)"
		[[ "$pin" == "$uref"* ]] || die "units.json is from reference $uref, the pinned reference is ${pin:0:12}: re-export the tables (README, Unreal client)"
		if [[ "${TC_PACKAGE_SKIP_TABLE_CHECK:-0}" != 1 ]]; then
			if command -v node >/dev/null; then
				node "$HERE/export-build-table.mjs" "$REF" --check >/dev/null || die "Client/TCConstructionData.inl is not what reference $uref gives (node unreal/Tools/export-build-table.mjs)"
				node "$HERE/export-ability-table.mjs" "$REF" --check >/dev/null || die "the ability tables are not what reference $uref gives (node unreal/Tools/export-ability-table.mjs reference/ww2-rts)"
				say "generated tables: units.json, construction and ability tables match reference $uref"
			else
				warn "node not found: the construction and ability tables were not compared with the reference"
			fi
		fi
	else
		warn "reference/ww2-rts is not checked out: units.json ($uref) not compared with the pin"
	fi

	if pgrep -f 'AutomationTool\.dll|UnrealBuildTool\.dll' >/dev/null 2>&1; then
		die "another Unreal build is running (UnrealBuildTool or AutomationTool): wait for it, two engine builds at once are not supported here"
	fi
}

mirror() {
	if [[ "${TC_PACKAGE_CLEAN:-0}" == 1 ]]; then say "TC_PACKAGE_CLEAN=1: removing $MIRROR"; rm -rf "$MIRROR"; fi
	mkdir -p "$MIRROR"
	# only what the game is made from; the copy's own Binaries, Intermediate, Saved and DerivedDataCache are kept for
	# an incremental rebuild (rsync leaves excluded folders alone on the receiving side)
	rsync -a --delete \
		--exclude=/Binaries --exclude=/Intermediate --exclude=/Saved --exclude=/DerivedDataCache --exclude=/Tools \
		--exclude='/Plugins/*/Binaries' --exclude='/Plugins/*/Intermediate' --exclude='*.code-workspace' \
		"$PROJECT_DIR/" "$MIRROR/"
	say "project copied to $MIRROR"
}

source_digest() { # one hash over the files the package is made from (paths and contents)
	(cd "$MIRROR" && find . -type f \
		-not -path './Binaries/*' -not -path './Intermediate/*' -not -path './Saved/*' -not -path './DerivedDataCache/*' \
		-not -path './Plugins/*/Binaries/*' -not -path './Plugins/*/Intermediate/*' -print0 | LC_ALL=C sort -z | xargs -0 sha256sum) | sha256sum | cut -d' ' -f1
}

uat() {
	local logs="$1" archive="$2"
	mkdir -p "$logs/uat"
	export TC_REQUIRE_WW2_FFI=1    # WW2Core.Build.cs: a missing Rust core is a build error, not the interim port
	export uebp_LogFolder="$logs/uat"
	local debug=(-nodebuginfo); [[ "${TC_PACKAGE_DEBUGINFO:-0}" == 1 ]] && debug=()
	local cmd=("$RUNUAT" BuildCookRun -project="$MIRROR/$PROJECT_NAME.uproject" -target="$PROJECT_NAME" -platform=Linux
		-clientconfig=Development -build -cook -stage -pak -iostore -compressed -package -archive
		-stagingdirectory="$WORK/staged" -archivedirectory="$archive" "${debug[@]}"
		-unattended -utf8output -nop4 -ubtargs="-MaxParallelActions=$JOBS")
	printf '%q ' "${cmd[@]}" >"$logs/uat-command.txt"; echo >>"$logs/uat-command.txt"
	say "RunUAT BuildCookRun (log $logs/uat.log, $JOBS compile processes; shader compiling keeps the engine's default of half the cores free)"
	set +e
	"${cmd[@]}" >"$logs/uat.log" 2>&1
	local rc=$?
	set -e
	if (( rc != 0 )); then
		grep -E 'error|Error|ERROR' "$logs/uat.log" | grep -v 'Warning' | tail -n 25 >&2 || true
		die "BuildCookRun failed (exit $rc): $logs/uat.log, engine logs in $logs/uat"
	fi
	say "BuildCookRun succeeded"
}

stamp_sha() { # the STAMP's sha256 for a fixture file name (ww2-ffi.sh: '-' and '.' become '_', then _sha256)
	local key; key="$(echo "$1" | tr -- '-.' '__')_sha256"
	sed -n "s/^$key=//p" "$FFI_DIR/STAMP"
}

# Copies the self-check stream (messages and oracle) into a package, checked against the install's STAMP first.
stage_fixtures() {
	local pkg="$1" dest="$1/Linux/$PROJECT_NAME/$FIXTURE_REL" name want got
	[[ -d "$pkg/Linux/$PROJECT_NAME" ]] || die "stage-fixtures: $pkg is not a package folder (no Linux/$PROJECT_NAME)"
	[[ -f "$FFI_DIR/STAMP" ]] || die "stage-fixtures: no ww2-ffi install at $FFI_DIR (run unreal/Tools/ww2-ffi.sh build)"
	mkdir -p "$dest"
	for name in "$SELFCHECK_STREAM.jsonl" "$SELFCHECK_STREAM.expected.jsonl"; do
		[[ -f "$FFI_DIR/fixtures/protocol/$name" ]] || die "stage-fixtures: $FFI_DIR/fixtures/protocol/$name is missing (run unreal/Tools/ww2-ffi.sh build)"
		want="$(stamp_sha "$name")"
		[[ -n "$want" ]] || die "stage-fixtures: the STAMP has no sha256 for $name"
		got="$(sha256sum "$FFI_DIR/fixtures/protocol/$name" | cut -d' ' -f1)"
		[[ "$got" == "$want" ]] || die "stage-fixtures: $name in the install does not match its STAMP (run unreal/Tools/ww2-ffi.sh build)"
		install -m 644 "$FFI_DIR/fixtures/protocol/$name" "$dest/$name"
	done
	say "staged the Rust view's self-check stream ($SELFCHECK_STREAM, $(du -ch "$dest"/*.jsonl | tail -n 1 | cut -f1)) at Linux/$PROJECT_NAME/$FIXTURE_REL"
}

# Static checks of a package folder. Prints what it found; stops at the first failure.
verify() {
	local pkg="$1"
	local root="$pkg/Linux" bin="$pkg/Linux/$PROJECT_NAME/Binaries/Linux/$PROJECT_NAME"
	[[ -x "$bin" ]] || die "verify: no executable $bin"
	file -b "$bin" | grep -q 'ELF 64-bit LSB .*x86-64' || die "verify: $bin is not an x86-64 ELF executable ($(file -b "$bin"))"
	say "verify: executable $(du -h "$bin" | cut -f1) $(file -b "$bin" | cut -d, -f1-2)"
	local missing; missing="$(ldd "$bin" | awk '/not found/ {print $1}' | tr '\n' ' ' || true)"
	[[ -z "$missing" ]] || die "verify: unresolved shared libraries: $missing"
	say "verify: every shared library resolves ($(ldd "$bin" | wc -l) entries)"
	# game target, not the editor module: a monolithic executable with the project module and the plugin linked in
	[[ ! -e "$pkg/Linux/$PROJECT_NAME/Binaries/Linux/libUnrealEditor-$PROJECT_NAME.so" ]] || die "verify: an editor module is in the package"
	if ls "$pkg/Linux/$PROJECT_NAME/Binaries/Linux/"*.so >/dev/null 2>&1; then die "verify: project shared libraries in the package (expected one monolithic executable)"; fi
	# the Rust core: the C entry points are in the executable, and WW2Core was compiled with the library (its STAMP's
	# native commit is compiled in only when WITH_WW2_FFI=1)
	local rev; rev="$(sed -n 's/^native_rev=//p' "$FFI_DIR/STAMP" 2>/dev/null || true)"
	# (the executable exports its symbols dynamically, as Unreal links it; strings is the fallback for a stripped one)
	local syms; syms="$({ nm -D --defined-only "$bin" 2>/dev/null || true; } | awk '$2 == "T" && $3 ~ /^ww2_/ {print $3}' | sort -u | tr '\n' ' ')"
	if [[ -z "$syms" ]]; then
		syms="$({ strings -a "$bin" || true; } | grep -oE '\bww2_(abi_version|rules_hash|last_error|formation_slot_size|formation_spots)\b' | sort -u | tr '\n' ' ' || true)"
		[[ -n "$syms" ]] && syms="$syms(from strings: no dynamic symbols)"
	fi
	[[ "$syms" == *ww2_abi_version* && "$syms" == *ww2_formation_spots* ]] || die "verify: no ww2_* entry points in the executable: the Rust core is not linked"
	say "verify: Rust core entry points: $syms"
	if [[ -n "$rev" ]]; then
		{ strings -a -el "$bin" || true; } | grep -qF "$rev" || die "verify: WW2Core was not compiled with ww2-ffi native $rev (WITH_WW2_FFI=0?)"
		say "verify: WW2Core compiled against ww2-ffi native ${rev:0:12}"
	fi
	# cooked content and the staged unit table
	local paks=("$root/$PROJECT_NAME/Content/Paks/"*.pak)
	[[ -f "${paks[0]}" ]] || die "verify: no .pak in $root/$PROJECT_NAME/Content/Paks"
	ls "$root/$PROJECT_NAME/Content/Paks/"*.utoc >/dev/null 2>&1 || die "verify: no IoStore container (.utoc) in $root/$PROJECT_NAME/Content/Paks"
	local list="$WORK/pak-list.txt"; mkdir -p "$WORK"
	: >"$list"
	local p; for p in "${paks[@]}"; do "$UE/Engine/Binaries/Linux/UnrealPak" "$p" -List >>"$list" 2>&1 || die "verify: UnrealPak could not list $p"; done
	grep -q "$PROJECT_NAME/Content/Data/units.json" "$list" || die "verify: Content/Data/units.json is not in the pak (DirectoriesToAlwaysStageAsUFS)"
	grep -q "$PROJECT_NAME/Config/DefaultGame.ini\|$PROJECT_NAME/Config/" "$list" || warn "verify: no project config listed in the pak"
	say "verify: the pak holds Content/Data/units.json (full listing: $list)"
	# what the client loads by path at run time must be in the cook (the staging manifest UAT wrote)
	local manifest="$pkg/Manifest_UFSFiles_Linux.txt" need
	[[ -f "$manifest" ]] || die "verify: $manifest is missing"
	for need in ThreeCrossroads/Content/Data/units.json Engine/Content/Maps/Entry.umap Engine/Content/BasicShapes/Cube.uasset \
		Engine/Content/BasicShapes/Cylinder.uasset Engine/Content/BasicShapes/Cone.uasset Engine/Content/BasicShapes/BasicShapeMaterial.uasset \
		Engine/Content/EngineFonts/Roboto.uasset; do
		grep -q "^$need"$'\t' "$manifest" || die "verify: $need is not in the package (see the cook settings in Config/DefaultGame.ini)"
	done
	ls "$root/$PROJECT_NAME/Content/Paks/"*.utoc >/dev/null 2>&1 && grep -q '^ThreeCrossroads/Content/ShaderArchive-ThreeCrossroads-' "$manifest" \
		|| die "verify: no project shader archive in the package"
	say "verify: the cook holds the Entry map, the engine shapes and materials the client loads, the font and the shader archives"
	# content found at run time by path only (slices 10 to 12): the reference's models, soldiers and far figures, the
	# light, the overlay rings (slice 12), the map pieces' materials, the effects, the sounds and the Spanish dictionary
	for need in ThreeCrossroads/Content/Models/ModelCatalog.json ThreeCrossroads/Content/Models/Infantry/Source/kits.json \
		ThreeCrossroads/Content/Fonts/BarlowSemiCondensed/BarlowSemiCondensed-Medium.ttf ThreeCrossroads/Content/Fonts/BarlowSemiCondensed/BarlowSemiCondensed-Bold.ttf \
		ThreeCrossroads/Content/Fonts/BarlowSemiCondensed/BarlowSemiCondensed-SemiBold.ttf \
		ThreeCrossroads/Content/Models/Motion/FarFigures.bin ThreeCrossroads/Content/Data/locale-es.json \
		ThreeCrossroads/Content/Models/Motion/M_InfantryFar.uasset ThreeCrossroads/Content/Models/Motion/M_InfantryCrowdFaded.uasset \
		ThreeCrossroads/Content/Models/Infantry/SM_InfantryCrowdBody.uasset ThreeCrossroads/Content/Models/Materials/Look/M_TCLookPost.uasset \
		ThreeCrossroads/Content/Models/Materials/Look/M_TCGroundLambert.uasset ThreeCrossroads/Content/Models/Materials/Look/M_TCLookLine.uasset \
		ThreeCrossroads/Content/Models/Materials/Look/M_TCLookRing.uasset ThreeCrossroads/Content/Models/Materials/Look/MPC_TCLook.uasset ThreeCrossroads/Content/World/Materials/M_TCWorldSurface.uasset \
		ThreeCrossroads/Content/World/Materials/M_TCWorldLeaf.uasset ThreeCrossroads/Content/Effects/Materials/M_TCFxSprite.uasset \
		ThreeCrossroads/Content/Effects/Textures/T_FxAtlas.uasset ThreeCrossroads/Content/Audio/SFX/tankgun.uasset \
		ThreeCrossroads/Content/Relief/M_TCRelief.uasset ThreeCrossroads/Content/Terrain/Materials/MF_TCGroundPaint.uasset \
		ThreeCrossroads/Content/Terrain/Materials/M_TCGroundPaint.uasset ThreeCrossroads/Content/Terrain/Materials/M_TCCraterDecal.uasset \
		ThreeCrossroads/Content/Terrain/Materials/M_TCRubbleDecal.uasset ThreeCrossroads/Content/Water/M_TCWater.uasset \
		ThreeCrossroads/Content/Water/M_TCApron.uasset ThreeCrossroads/Content/Atmosphere/Materials/M_TCAtmosGround.uasset \
		ThreeCrossroads/Content/Atmosphere/Materials/M_TCAtmosMist.uasset ThreeCrossroads/Content/Atmosphere/Materials/M_TCAtmosFall.uasset \
		ThreeCrossroads/Content/Atmosphere/Materials/M_TCAtmosBird.uasset \
		ThreeCrossroads/Content/Models/Motion/WheelTracks.json ThreeCrossroads/Content/Models/Motion/M_TCModelTracks.uasset \
		ThreeCrossroads/Content/Models/Motion/Tracks/SM_medium_0_hull_Tracks.uasset ThreeCrossroads/Content/Models/Motion/M_InfantryCrowdLookFaded.uasset \
		ThreeCrossroads/Content/Models/Motion/MI_InfantryCrowdLook_DE.uasset ThreeCrossroads/Content/Models/Motion/M_InfantryCrowdKitLookFaded.uasset \
		ThreeCrossroads/Content/WarMap/WarMapData.json ThreeCrossroads/Content/WarMap/T_TCWarMapSymbols.uasset \
		ThreeCrossroads/Content/Markers/M_TCMarkerGround.uasset ThreeCrossroads/Content/Markers/M_TCMarkerThrough.uasset ThreeCrossroads/Content/Markers/T_TCUnitBadges.uasset \
		ThreeCrossroads/Content/World/Materials/Instances/MI_TCWorld_Surface_canvas_Painted.uasset ThreeCrossroads/Content/World/Materials/Instances/MI_TCWorld_Surface_darkwood_Painted.uasset \
		ThreeCrossroads/Content/Effects/Materials/M_TCFxSection.uasset \
		ThreeCrossroads/Content/Models/Motion/MI_InfantryFarLook.uasset ThreeCrossroads/Content/Models/Motion/MI_InfantryFarLookFaded.uasset \
		ThreeCrossroads/Content/WarMap/Fonts/F_TCWarMapMono.uasset; do
		grep -q "^$need"$'\t' "$manifest" || die "verify: $need is not in the package (see the cook settings in Config/DefaultGame.ini)"
	done
	local meshes; meshes="$(grep -c '^ThreeCrossroads/Content/Models/Meshes/.*\.uasset'$'\t' "$manifest" || true)"
	(( meshes > 0 )) || die "verify: no model meshes (Content/Models/Meshes) in the package"
	say "verify: the cook holds the models ($meshes meshes), soldiers, far figures, light, rings, world and effects materials, the terrain's relief, paint, water, apron and atmosphere materials (slice 13), the wheel and track hulls, the near crowd's look and the paper war map (round 14), sounds and the Spanish dictionary"
	# the Rust view's startup self-check replays this stream; without it the replay is skipped (the edge checks still
	# run), so a package must carry it, unchanged from the install it was built with
	local fx="$root/$PROJECT_NAME/$FIXTURE_REL" name
	for name in "$SELFCHECK_STREAM.jsonl" "$SELFCHECK_STREAM.expected.jsonl"; do
		[[ -f "$fx/$name" ]] || die "verify: $fx/$name is missing (the Rust view's self-check stream: tc-package-linux.sh stage-fixtures)"
		[[ "$(sha256sum "$fx/$name" | cut -d' ' -f1)" == "$(stamp_sha "$name")" ]] || die "verify: $fx/$name does not match the ww2-ffi STAMP"
	done
	say "verify: the Rust view's self-check stream ($SELFCHECK_STREAM) is staged and matches the ww2-ffi STAMP"
	[[ -x "$pkg/tc-client.sh" ]] || die "verify: tc-client.sh is missing"
	say "verify: $pkg passed the static checks"
}

build() {
	preflight
	mirror
	local stamp; stamp="$(date -u +%Y%m%dT%H%M%SZ)"
	local logs="$WORK/logs/$stamp" archive="${TC_PACKAGE_ARCHIVE:-$WORK/archive}"
	mkdir -p "$logs"
	rm -rf "$archive" "$WORK/staged"
	local digest; digest="$(source_digest)"
	uat "$logs" "$archive"
	# UE 5.8 archives the staged game at the archive folder's top; older engines put it in a Linux/ folder below
	local staged="$archive"; [[ -d "$archive/Linux/$PROJECT_NAME" ]] && staged="$archive/Linux"
	[[ -x "$staged/$PROJECT_NAME/Binaries/Linux/$PROJECT_NAME" ]] || die "BuildCookRun left no executable in $staged/$PROJECT_NAME/Binaries/Linux"
	grep -q "WW2Core: building without the Rust core" "$logs/uat.log" && die "WW2Core was built without the Rust core (see $logs/uat.log)"

	local next="$OUT.next"
	rm -rf "$next"; mkdir -p "$next"
	cp -a --reflink=auto "$staged" "$next/Linux"
	stage_fixtures "$next"
	cp "$logs/uat/Manifest_UFSFiles_Linux.txt" "$logs/uat/Manifest_NonUFSFiles_Linux.txt" "$next/" 2>/dev/null || true
	rm -f "$next/Linux/Manifest_UFSFiles_Linux.txt" "$next/Linux/Manifest_NonUFSFiles_Linux.txt"
	install -m 755 "$HERE/linux-package/tc-client.sh" "$next/tc-client.sh"
	install -m 644 "$HERE/linux-package/README.txt" "$next/README.txt"
	# round 14: the paper war map's font face (Liberation Mono Bold, SIL Open Font License 1.1) ships with its licence
	install -d "$next/licenses"
	install -m 644 "$HERE/../Content/WarMap/Fonts/LICENSE-LiberationMono-Bold.txt" "$next/licenses/LICENSE-LiberationMono-Bold.txt"
	{
		echo "Three Crossroads native client, Linux x86_64, Development package"
		echo "built_utc=$stamp"
		echo "engine=$(grep -o '"MajorVersion": *[0-9]*\|"MinorVersion": *[0-9]*\|"PatchVersion": *[0-9]*' "$UE/Engine/Build/Build.version" | grep -o '[0-9]*$' | paste -sd.) changelist $(grep -o '"Changelist": *[0-9]*' "$UE/Engine/Build/Build.version" | grep -o '[0-9]*$')"
		echo "toolchain=$("$(toolchain)/bin/clang++" --version | head -n 1)"
		echo "native_head=$(git -C "$REPO" rev-parse HEAD 2>/dev/null || echo unknown)"
		echo "native_worktree=$( [[ -n "$(git -C "$REPO" status --porcelain -- unreal 2>/dev/null)" ]] && echo 'uncommitted changes under unreal/ (see source_sha256)' || echo clean)"
		echo "source_sha256=$digest"
		echo "reference=$(git -C "$REF" rev-parse HEAD 2>/dev/null || echo unknown)"
		echo "units_json_reference=$(sed -n 's/^ *"reference": *"\([0-9a-f]*\)".*/\1/p' "$PROJECT_DIR/Content/Data/units.json" | head -n 1)"
		echo "ww2_ffi_stamp:"; sed 's/^/  /' "$FFI_DIR/STAMP"
		echo "server=not included: the temporary pinned JavaScript reference server (reference/ww2-rts, Node.js), started from a ww2-rts-native checkout with unreal/Tools/tc-local.sh serve. The Rust ww2-server does not exist yet."
		echo "uat_command=$(cat "$logs/uat-command.txt")"
	} >"$next/BUILD-INFO.txt"
	(cd "$next" && find . -type f -not -name SHA256SUMS -print0 | LC_ALL=C sort -z | xargs -0 sha256sum >SHA256SUMS)
	verify "$next"
	# replace the old package only now, so a failed build never leaves a half package at $OUT
	if [[ -e "$OUT" ]]; then rm -rf "$OUT.old"; mv "$OUT" "$OUT.old"; fi
	mv "$next" "$OUT"
	rm -rf "$OUT.old"
	say "package: $OUT ($(du -sh "$OUT" | cut -f1)); start it with $OUT/tc-client.sh --server ws://127.0.0.1:PORT/ws"
	say "logs: $logs"
}

case "${1:-}" in
	check) preflight; say "prerequisites met" ;;
	build) build ;;
	verify) verify "${2:-$OUT}" ;;
	stage-fixtures)
		pkg="${2:-$OUT}"
		stage_fixtures "$pkg"
		# the SHA256SUMS of a built package lists every file: add the two staged ones
		if [[ -f "$pkg/SHA256SUMS" ]]; then (cd "$pkg" && find . -type f -not -name SHA256SUMS -print0 | LC_ALL=C sort -z | xargs -0 sha256sum >SHA256SUMS); say "SHA256SUMS rewritten"; fi
		verify "$pkg" ;;
	*) sed -n '2,27p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
