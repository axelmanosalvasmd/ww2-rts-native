#!/usr/bin/env bash
# Plays one reference mode end to end against an isolated reference server (unreal/Tools/tc-local.sh serve), with the
# Unreal client: a rendered native host "Alder" (English, 1280x720 offscreen) makes the room, waits for the native guest
# "Fern" (Spanish, headless, seated as a player), sets the mode and map as the reference lobby sends them ({t:'mode'},
# {t:'map'}), adds the AI seats, sets the teams and starts; then the match plays until its own end (the epilogue, then
# the lobby with the result and the report) or the mode's time cap. Captures: the lobby, mid-match views, the epilogue
# stamp, the result card, and (Tutorial) one per step. TC_DECODER=rust on both clients. Nothing here spawns a unit.
#
#   usage: run-mode.sh <mode id from modes.json> <out dir> [room]
# Environment:
#   TC_MODES_PROJECT  the .uproject to run (default: this checkout's unreal/ThreeCrossroads.uproject)
#   TC_RUN_DIR        the tc-local.sh server folder (its port file), or TC_MODES_URL=ws://127.0.0.1:<port>/ws
#   TC_MODES_CONFIG   optional explicit lane table (default modes.json beside this script)
#   TC_MODES_TOKENS   folder for the seat token files (required: tokens never go into the out dir)
#   TC_MODES_GRAPHICS high (default) or low, explicitly applied before every client; auto-drop disabled for the lane
#   UE                engine folder (default ~/Applications/UnrealEngine-5.8.3)
#   TC_MODES_CAP      seconds, overrides the mode's cap; TC_MODES_GUEST_RENDER=1 renders the guest too (captures in es;
#                     a mode's "guestRender": true in modes.json does the same)
# Writes into <out dir>: host.log, guest.log (the engine logs), host/ and guest/ captures, cmdlines.txt, exit.txt,
# timing.txt, and row.json + row.md from summarize.mjs.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TABLE="${TC_MODES_CONFIG:-$HERE/modes.json}"
ID="$1"; OUT="$2"; ROOM="${3:-m$(date +%H%M%S)$((RANDOM % 100))}"
UE="${UE:-$HOME/Applications/UnrealEngine-5.8.3}"
P="${TC_MODES_PROJECT:-$(cd "$HERE/../.." && pwd)/ThreeCrossroads.uproject}"
T="${TC_MODES_TOKENS:?set TC_MODES_TOKENS to a folder outside the evidence}"
GRAPHICS="${TC_MODES_GRAPHICS:-high}"
[[ "$GRAPHICS" == high || "$GRAPHICS" == low ]] || { echo 'TC_MODES_GRAPHICS must be high or low' >&2; exit 2; }
# A previous lane can persist Low. Set the saved choice before launch, including on a fresh project, and use the
# command-line ini override for every client so a concurrent lane cannot change the choice it loads.
python3 - "$P" "$GRAPHICS" <<'PY'
import configparser, pathlib, sys
ini = pathlib.Path(sys.argv[1]).parent / 'Saved/Config/LinuxEditor/GameUserSettings.ini'
ini.parent.mkdir(parents=True, exist_ok=True)
c = configparser.RawConfigParser(strict=False); c.optionxform = str
if ini.exists(): c.read(ini)
if not c.has_section('ThreeCrossroads'): c.add_section('ThreeCrossroads')
c.set('ThreeCrossroads', 'Graphics', sys.argv[2]); c.set('ThreeCrossroads', 'GraphicsAutoDropped', '1')
c.set('ThreeCrossroads', 'UIScale', '100')
with ini.open('w') as f: c.write(f)
PY
PREFERENCES=(-ini:GameUserSettings:[ThreeCrossroads]:Graphics="$GRAPHICS" -ini:GameUserSettings:[ThreeCrossroads]:GraphicsAutoDropped=1 -ini:GameUserSettings:[ThreeCrossroads]:UIScale=100 -TCGraphicsAt="$GRAPHICS@0")
# Avoid the observed UE pre-init race between trace-worker logging and LLM teardown. These lanes do not record traces.
DIAGNOSTICS=(-trace=none -notracethreading -notraceserver -nollm)
if [ -n "${TC_MODES_URL:-}" ]; then URL="$TC_MODES_URL"; else URL="ws://127.0.0.1:$(cat "${TC_RUN_DIR:?set TC_RUN_DIR or TC_MODES_URL}/port")/ws"; fi
mkdir -p "$OUT/host" "$OUT/guest" "$T"
CFG=$(python3 - "$TABLE" "$ID" "${TC_MODES_CAP:-}" <<'PY'
import json, sys, shlex
table, mid, cap = sys.argv[1], sys.argv[2], sys.argv[3]
c = json.load(open(table))[mid]
cap = int(cap) if cap else c["cap"]
shots = c.get("shots", [])
steps, prev = ["6@lobby:shot:modes-%s-lobby" % mid], None
for k, s in enumerate(shots):
    tag = "objectives" if k == 0 else "match-%ds" % s
    steps.append(("%d@play:shot:modes-%s-%s" % (s, mid, tag)) if prev is None else ("%d:shot:modes-%s-%s" % (s - prev, mid, tag)))
    prev = s
if c.get("script"): steps.append(c["script"])
out = {"MODE": c["mode"], "MAP": c.get("map") or "", "AI": str(c.get("ai", 0)), "GUEST": "1" if c.get("guest") else "0",
       "TEAMS": ",".join(str(t) for t in c.get("teams", [])), "CAPTURE": "1" if c.get("capturePlay") else "0",
       "DRIVE": "1" if c.get("drive") else "0", "CAP": str(cap), "ENDATCAP": "1" if c.get("endAtCap") else "0", "SANDBOX": "1" if c.get("sandbox") else "0", "GUESTRENDER": "1" if c.get("guestRender") else "0", "TOUR": c.get("tour") or "", "SPECTATOR": "1" if c.get("spectator") else "0",
       "SCRIPT": ";".join(steps)}
for k, v in out.items(): print("%s=%s" % (k, shlex.quote(v)))
PY
) || { echo "run-mode: no mode '$ID' in modes.json" >&2; exit 2; }
eval "$CFG"
SEATS=1; [ "$GUEST" = 1 ] && SEATS=2
# -TCHostStart=N adds AI seats until the lobby holds 1 + N seats: the guest counts among them
HOSTSTART=$((AI + SEATS - 1))
HOST=(env TC_DECODER=rust SDL_AUDIODRIVER=dummy SDL_AUDIO_DRIVER=dummy "$UE/Engine/Binaries/Linux/UnrealEditor" "$P" -game -RenderOffscreen -ResX=1280 -ResY=720 -unattended -nosplash
  "${PREFERENCES[@]}" "${DIAGNOSTICS[@]}"
  -TCServer="$URL" -TCRoom="$ROOM" -TCName=Alder -TCTokenFile="$T/host-$ROOM.txt" -TCLang=en -TCObjectivesLog
  -TCHostStart="$HOSTSTART" -TCHostMode="$MODE" -TCHostSeats="$SEATS"
  -TCControlsCheck="$SCRIPT" -TCControlsShotDir="$OUT/host"
  -TCEpilogueShotFile="$OUT/host/modes-$ID-epilogue.png" -TCResultShotFile="$OUT/host/modes-$ID-end-report.png" -TCExitAfterResult=8
  -TCExitAfter=$((CAP + 300)) -abslog="$OUT/host.log")
[ -n "$MAP" ] && HOST+=(-TCHostMap="$MAP")
[ -n "$TEAMS" ] && HOST+=(-TCHostTeams="$TEAMS")
[ "$CAPTURE" = 1 ] && HOST+=(-TCCapturePlay)
[ "$DRIVE" = 1 ] && HOST+=(-TCTutorialDrive -TCTutorialShots="$OUT/host" -TCTutorialShotTag=en)
# at the cap the host ends the match (server.js 'end', the menu's End match): the room goes back to the lobby with
# "Match ended by the host" and no report, and both clients leave after it; the row then says the cap was reached
HOST+=(-TCEndAfter="$CAP")
[ "$SANDBOX" = 1 ] && HOST+=(-TCHostSandbox)
# a camera tour (tours/<name>.tour, Client/TCSlice13Checks.cpp): poses at match times, each saved as <name>.png; with
# "spectator": true a third native (rendered, English) watches as a spectator and flies the tour instead: a spectator sees
# every cell (server.js watchGame), where a seat only learns of a change its units see (shared/sim.js observedTerrainFor)
SPECCMD=(env TC_DECODER=rust SDL_AUDIODRIVER=dummy SDL_AUDIO_DRIVER=dummy "$UE/Engine/Binaries/Linux/UnrealEditor" "$P" -game -RenderOffscreen -ResX=1280 -ResY=720 -unattended -nosplash
  "${PREFERENCES[@]}" "${DIAGNOSTICS[@]}"
  -TCServer="$URL" -TCRoom="$ROOM" -TCName=Wren -TCSpectate -TCTokenFile="$T/spectator-$ROOM.txt" -TCLang=en -TCObjectivesLog -TCExitAfterResult=6
  -TCExitAfter=$((CAP + 300)) -abslog="$OUT/spectator.log")
if [ -n "$TOUR" ] && [ "$SPECTATOR" = 1 ]; then mkdir -p "$OUT/spectator"; SPECCMD+=(-TCTourFile="$HERE/tours/$TOUR" -TCTourDir="$OUT/spectator")
elif [ -n "$TOUR" ]; then HOST+=(-TCTourFile="$HERE/tours/$TOUR" -TCTourDir="$OUT/host"); fi
[ "$GUESTRENDER" = 1 ] && TC_MODES_GUEST_RENDER=1
if [ "${TC_MODES_GUEST_RENDER:-0}" = 1 ]; then GR=(-RenderOffscreen -ResX=1280 -ResY=720 -TCEpilogueShotFile="$OUT/guest/modes-$ID-epilogue-es.png" -TCResultShotFile="$OUT/guest/modes-$ID-end-report-es.png"); else GR=(-nullrhi -nosound); fi
GUESTCMD=(env TC_DECODER=rust SDL_AUDIODRIVER=dummy SDL_AUDIO_DRIVER=dummy "$UE/Engine/Binaries/Linux/UnrealEditor" "$P" -game "${GR[@]}" -unattended -nosplash
  "${PREFERENCES[@]}" "${DIAGNOSTICS[@]}"
  -TCServer="$URL" -TCRoom="$ROOM" -TCName=Fern -TCTokenFile="$T/guest-$ROOM.txt" -TCLang=es -TCObjectivesLog -TCExitAfterResult=6
  -TCExitAfter=$((CAP + 300)) -abslog="$OUT/guest.log")
[ "$CAPTURE" = 1 ] && GUESTCMD+=(-TCCapturePlay)
[ "$DRIVE" = 1 ] && { GUESTCMD+=(-TCTutorialDrive); [ "${TC_MODES_GUEST_RENDER:-0}" = 1 ] && GUESTCMD+=(-TCTutorialShots="$OUT/guest" -TCTutorialShotTag=es); }
{ echo "mode id: $ID; room $ROOM; server $URL; decoder TC_DECODER=rust"; echo "graphics: $GRAPHICS"; echo "config: $(python3 -c "import json,sys; print(json.dumps(json.load(open('$TABLE'))['$ID']))")";
  so="$(dirname "$P")/Binaries/Linux/libUnrealEditor-ThreeCrossroads.so"; [ -f "$so" ] && echo "module: $(stat -c '%y, %s bytes' "$so"), md5 $(md5sum "$so" | cut -c1-12)"
  echo "host: ${HOST[*]}"; [ "$GUEST" = 1 ] && echo "guest: ${GUESTCMD[*]}"; [ "$SPECTATOR" = 1 ] && echo "spectator: ${SPECCMD[*]}"; } > "$OUT/cmdlines.txt"
echo "begin $(date -u +%FT%TZ)" > "$OUT/timing.txt"
"${HOST[@]}" > "$OUT/host-console.log" 2>&1 & H=$!
echo "host pid $H" >> "$OUT/timing.txt"
G=""
if [ "$GUEST" = 1 ]; then
  # the host must make the room first (the first seat is the host)
  for i in $(seq 1 360); do grep -q "LogTC: lobby: room $ROOM" "$OUT/host.log" 2>/dev/null && break; kill -0 $H 2>/dev/null || break; sleep 0.5; done
  "${GUESTCMD[@]}" > "$OUT/guest-console.log" 2>&1 & G=$!
  echo "guest pid $G" >> "$OUT/timing.txt"
fi
W=""
if [ "$SPECTATOR" = 1 ]; then
  for i in $(seq 1 360); do grep -q "LogTC: lobby: room $ROOM" "$OUT/host.log" 2>/dev/null && break; kill -0 $H 2>/dev/null || break; sleep 0.5; done
  "${SPECCMD[@]}" > "$OUT/spectator-console.log" 2>&1 & W=$!
  echo "spectator pid $W" >> "$OUT/timing.txt"
fi
# a guest that dies before the match starts (out of GPU memory at its first frame, say) would leave the host waiting in
# the lobby for the second seat until -TCExitAfter: stop the host 30 s after such a death and say so
GD=0
while kill -0 $H 2>/dev/null; do
  if [ -n "$G" ] && ! kill -0 $G 2>/dev/null && ! grep -q "LogTC: start #" "$OUT/host.log" 2>/dev/null; then
    GD=$((GD + 1)); [ $GD -ge 15 ] && { echo "guest died before the match started; host stopped by the driver" >> "$OUT/exit.txt"; kill $H; break; }
  fi
  sleep 2
done
wait $H; echo "host exit $?" >> "$OUT/exit.txt"
if [ -n "$G" ]; then
  # the guest leaves on its own after the result (or at its cap); give it a minute after the host
  for i in $(seq 1 120); do kill -0 $G 2>/dev/null || break; sleep 0.5; done
  kill -0 $G 2>/dev/null && { kill $G; echo "guest stopped by the driver 60 s after the host" >> "$OUT/exit.txt"; }
  wait $G; echo "guest exit $?" >> "$OUT/exit.txt"
fi
if [ -n "$W" ]; then
  for i in $(seq 1 120); do kill -0 $W 2>/dev/null || break; sleep 0.5; done
  kill -0 $W 2>/dev/null && { kill $W; echo "spectator stopped by the driver 60 s after the host" >> "$OUT/exit.txt"; }
  wait $W; echo "spectator exit $?" >> "$OUT/exit.txt"
fi
echo "end $(date -u +%FT%TZ)" >> "$OUT/timing.txt"
node "$HERE/summarize.mjs" --id "$ID" --cap "$CAP" "$OUT" > "$OUT/row.md" 2> "$OUT/summarize.err"
cat "$OUT/row.md"
# A rendered table is not proof of a successful lane: preserve the row and fail on a missing start or client failure.
python3 - "$OUT/row.json" <<'PY'
import json, sys
row = json.load(open(sys.argv[1]))
if not row.get('started') or any(value != 0 for key, value in row.get('exits', {}).items() if key in ('host', 'guest', 'spectator')):
    sys.exit(1)
PY
