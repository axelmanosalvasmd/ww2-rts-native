#!/usr/bin/env bash
# Great Bridge tick-8400 Unreal decoder replay (ThreeCrossroads.Net.LegacyBridgeReplay), standalone runner.
#
#   run.sh <bundle.tar.gz> <work dir> [out dir]
#
# Verifies the bundle (bundle.py verify: every manifest asset's bytes and SHA-256), extracts it into <work dir>
# (never into the repository), prints bundle.py's independent summary, then runs the automation test on an editor
# build with TC_BRIDGE_REPLAY_DIR pointing at the extracted folder. Results go to <out dir> (default <work dir>/out):
# summary.txt, <stream>.summary.txt, <stream>.state-8400.txt, compare-slot0.txt, compare-slot1.txt, test.log.
#
# Environment: TC_PROJECT (the .uproject; default this checkout's unreal/ThreeCrossroads.uproject), UE_ROOT (the
# engine; default ~/Applications/UnrealEngine-5.8.3), TC_BUNDLE_SHA256 (optional: the archive's expected
# SHA-256). The editor target must be built with the WW2Core Rust library (TC_REQUIRE_WW2_FFI=1) for the Rust view half.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
BUNDLE=${1:?usage: run.sh <bundle.tar.gz> <work dir> [out dir]}
WORK=${2:?usage: run.sh <bundle.tar.gz> <work dir> [out dir]}
OUT=${3:-$WORK/out}
PROJECT=${TC_PROJECT:-$(cd "$HERE/../.." && pwd)/ThreeCrossroads.uproject}
UE=${UE_ROOT:-$HOME/Applications/UnrealEngine-5.8.3}
mkdir -p "$WORK" "$OUT"
SUM=$(sha256sum "$BUNDLE" | cut -d' ' -f1)
echo "archive sha256 $SUM"
if [ -n "${TC_BUNDLE_SHA256:-}" ] && [ "$SUM" != "$TC_BUNDLE_SHA256" ]; then echo "archive hash differs from TC_BUNDLE_SHA256"; exit 1; fi
tar -xzf "$BUNDLE" -C "$WORK" || exit 1
DIR="$WORK/great-bridge-8400"
python3 "$HERE/bundle.py" verify "$DIR" | tee "$OUT/manifest-verify.txt"
[ "${PIPESTATUS[0]}" = 0 ] || { echo "manifest verification failed: not running the replay"; exit 1; }
python3 "$HERE/bundle.py" summary "$DIR" | tee "$OUT/bundle-summary.txt"
TC_BRIDGE_REPLAY_DIR="$DIR" TC_BRIDGE_REPLAY_OUT="$OUT" timeout 1800 "$UE/Engine/Binaries/Linux/UnrealEditor-Cmd" "$PROJECT" -nullrhi -unattended -nosplash -nop4 -NoSound \
  -ExecCmds="Automation RunTests ThreeCrossroads.Net.LegacyBridgeReplay; Quit" -abslog="$OUT/test.log" > /dev/null 2>&1
echo "editor exit $?"
grep -E "Test Completed\. Result=\{[A-Za-z]+\}.*LegacyBridgeReplay" "$OUT/test.log" | sed -E 's/.*Result=\{([A-Za-z]+)\}.*/test result: \1/'
grep -cE "LogAutomationController: Error" "$OUT/test.log" | sed 's/^/automation errors: /'
[ -f "$OUT/summary.txt" ] && cat "$OUT/summary.txt"
