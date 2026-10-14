#!/usr/bin/env bash
# The tick-8400 view of the Great Bridge in the game itself, from a recorded recipient stream (no reference server, no
# simulation): replay-server.mjs serves the stream to one rendered client over a real local WebSocket, holding the
# tick-8400 snapshot back for --release-after seconds; the client's atmosphere check (-TCAtmosShotAfter, Focus, Dist)
# looks at the bridge and saves frames with and without the HUD at the given seconds after its first match frame.
#
#   capture.sh <extracted great-bridge-8400 dir> <stream stem> <out dir> <lang en|es> [shot seconds, default 45,56,59.5,63]
#
# Environment: TC_PROJECT (default this checkout's unreal/ThreeCrossroads.uproject), UE_ROOT, TC_RELEASE_AFTER (60),
# TC_FOCUS (119,98: the bridge's middle in metres), TC_DIST (70). Writes <out>/server.log, <out>/game.log and
# <out>/bridge-<stem>-<lang>-<S>s-{nohud,hud}.png. The client is stopped by its own -TCExitAfter.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
DIR=${1:?usage}; STEM=${2:?usage}; OUT=${3:?usage}; LANG_=${4:?usage}; SHOTS=${5:-45,56,59.5,63}
PROJECT=${TC_PROJECT:-$(cd "$HERE/../.." && pwd)/ThreeCrossroads.uproject}
UE=${UE_ROOT:-$HOME/Applications/UnrealEngine-5.8.3}
mkdir -p "$OUT"
rm -f "$OUT/port"
node "$HERE/replay-server.mjs" --stream "$DIR/streams/$STEM.jsonl" --port-file "$OUT/port" --release-after "${TC_RELEASE_AFTER:-60}" --log "$OUT/server.log" &
SERVER=$!
for _ in $(seq 50); do [ -s "$OUT/port" ] && break; sleep 0.1; done
PORT=$(cat "$OUT/port")
echo "server pid $SERVER port $PORT"
env TC_DECODER=rust SDL_AUDIODRIVER=dummy SDL_AUDIO_DRIVER=dummy "$UE/Engine/Binaries/Linux/UnrealEditor" "$PROJECT" -game -RenderOffscreen -ResX=1280 -ResY=720 -unattended -nosplash \
  -TCServer="ws://127.0.0.1:$PORT/ws" -TCRoom=bridge -TCName=Alder -TCTokenFile="$OUT/token.txt" -TCLang="$LANG_" -TCObjectivesLog \
  -TCAtmosShotAfter="$SHOTS" -TCAtmosShotFocus="${TC_FOCUS:-119,98}" -TCAtmosShotDist="${TC_DIST:-70}" -TCAtmosShotFile="$OUT/bridge-$STEM-$LANG_.png" \
  -TCExitAfter=95 -abslog="$OUT/game.log" > /dev/null 2>&1
echo "game exit $?"
kill "$SERVER" 2>/dev/null; wait "$SERVER" 2>/dev/null
rm -f "$OUT/token.txt" "$OUT/port"
ls "$OUT"/*.png 2>/dev/null
