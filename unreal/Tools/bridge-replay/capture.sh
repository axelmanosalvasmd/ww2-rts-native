#!/usr/bin/env bash
# Four rendered replay frames, timed from tick 8400 receipt, on a scratch project.
# capture.sh <extracted bundle dir> <stream stem> <log dir> <en|es>
# TC_PROJECT points at the scratch uproject. TC_CAPTURE_WORK is scratch space for port and token.
# TC_SHOT_PREFIX selects the PNG prefix (default <log dir>/bridge-<stream>-<lang>).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
DIR=${1:?usage}; STEM=${2:?usage}; OUT=${3:?usage}; LANGUAGE=${4:?usage}
PROJECT=${TC_PROJECT:-$(cd "$HERE/../.." && pwd)/ThreeCrossroads.uproject}
UE=${UE_ROOT:-$HOME/Applications/UnrealEngine-5.8.3}
WORK=${TC_CAPTURE_WORK:?set TC_CAPTURE_WORK to a scratch directory}
PREFIX=${TC_SHOT_PREFIX:-$OUT/bridge-$STEM-$LANGUAGE}
mkdir -p "$OUT" "$WORK" "$(dirname "$PREFIX")"
for NAME in before at after1s after3s; do
  [ ! -e "$PREFIX-$NAME.png" ] || { echo "Capture already exists: $PREFIX-$NAME.png. Choose a fresh prefix."; exit 1; }
done
rm -f "$WORK/port"
SERVER=; GAME=
cleanup() {
  if [ -n "$GAME" ] && kill -0 "$GAME" 2>/dev/null; then kill "$GAME"; wait "$GAME" || true; fi
  if [ -n "$SERVER" ] && kill -0 "$SERVER" 2>/dev/null; then kill "$SERVER"; wait "$SERVER" || true; fi
  rm -f "$WORK/port" "$WORK/token.txt"
}
trap cleanup EXIT
# Explicit High before every lane; prevent auto-drop during a capture.
python3 - "$PROJECT" <<'PY'
import configparser, pathlib, sys
p = pathlib.Path(sys.argv[1]).parent / 'Saved/Config/LinuxEditor/GameUserSettings.ini'
p.parent.mkdir(parents=True, exist_ok=True)
c = configparser.RawConfigParser(strict=False); c.optionxform = str
if p.exists(): c.read(p)
if not c.has_section('ThreeCrossroads'): c.add_section('ThreeCrossroads')
c.set('ThreeCrossroads','Graphics','high'); c.set('ThreeCrossroads','GraphicsAutoDropped','1')
with p.open('w') as f: c.write(f)
PY
node "$HERE/replay-server.mjs" --stream "$DIR/streams/$STEM.jsonl" --port-file "$WORK/port" --release-after 600 --log "$OUT/server.log" &
SERVER=$!
for _ in $(seq 100); do [ -s "$WORK/port" ] && break; sleep 0.1; done
PORT=$(cat "$WORK/port")
echo "replay pid $SERVER port $PORT"
env TC_DECODER=rust SDL_AUDIODRIVER=dummy SDL_AUDIO_DRIVER=dummy "$UE/Engine/Binaries/Linux/UnrealEditor" "$PROJECT" -game -RenderOffscreen -ResX=1280 -ResY=720 -unattended -nosplash -nosound \
  -TCServer="ws://127.0.0.1:$PORT/ws" -TCRoom=bridge -TCName=Alder -TCTokenFile="$WORK/token.txt" -TCLang="$LANGUAGE" -TCObjectivesLog \
  -TCBridgeCapture="$PREFIX" -TCGraphicsAt=high@0 -TCExitAfter=40 -abslog="$OUT/game.log" > /dev/null 2>&1 &
GAME=$!
READY=0
for _ in $(seq 1200); do
  if [ -s "$PREFIX-before.png" ]; then READY=1; break; fi
  if ! kill -0 "$GAME" 2>/dev/null; then break; fi
  sleep 0.1
done
[ "$READY" = 1 ] || { echo 'No before capture; see game.log'; exit 1; }
curl -fsS -X POST "http://127.0.0.1:$PORT/replay/release"
wait "$GAME"
GAME=
for NAME in before at after1s after3s; do test -s "$PREFIX-$NAME.png"; done
rg 'bridgefx capture:|rust view decoder|Rust view shadow' "$OUT/game.log"
