#!/usr/bin/env bash
# Local play for the Unreal slice: an isolated copy of the pinned reference Node server on a free loopback port, and
# the Unreal client against it. Nothing here touches any other server, and nothing is sent off this machine.
#
#   unreal/Tools/tc-local.sh serve    copy reference/ww2-rts (its checked-out commit) and start it in the background
#   unreal/Tools/tc-local.sh health   the server this script started: process, HTTP and a WebSocket hello -> lobby
#   unreal/Tools/tc-local.sh status   where it runs (URL, pid, log)
#   unreal/Tools/tc-local.sh play     the client in a window: host room $TC_ROOM with $TC_AI AI seats and start
#   unreal/Tools/tc-local.sh host     the client in a window as host of $TC_ROOM, left in the lobby: Mode, Add AI and
#                                     Start match are buttons on the HUD
#   unreal/Tools/tc-local.sh join     the client in a window as a second player (seat label "guest") in that room
#   unreal/Tools/tc-local.sh watch    the client in a window as a spectator of that room
#   unreal/Tools/tc-local.sh check    a headless client (-nullrhi, no window) in a room of its own: start, move, quit
#   unreal/Tools/tc-local.sh stop     stop the server this script started (only that one)
#
# Environment (all optional):
#   UE          engine folder (default ~/Applications/UnrealEngine-5.8.3); the editor target must be built (README)
#   TC_RUN_DIR  server copy, pid, port and logs (default unreal/Saved/LocalServer, ignored by git)
#   TC_PORT     port to use instead of a free one; TC_ROOM (default native1); TC_NAME (default Commander); TC_AI (default 1)
#   TC_SEAT     seat label for play (default host). Each label keeps its own seat token in unreal/Saved/TCTokens/, so
#               two clients on one machine need two labels. Tokens never go on the command line or into the logs.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$HERE/.." && pwd)"
REPO="$(cd "$PROJECT_DIR/.." && pwd)"
REF="$REPO/reference/ww2-rts"
UE="${UE:-$HOME/Applications/UnrealEngine-5.8.3}"
RUN="${TC_RUN_DIR:-$PROJECT_DIR/Saved/LocalServer}"
ROOM="${TC_ROOM:-native1}"
NAME="${TC_NAME:-Commander}"
AI="${TC_AI:-1}"
EDITOR="$UE/Engine/Binaries/Linux/UnrealEditor"
UPROJECT="$PROJECT_DIR/ThreeCrossroads.uproject"

die() { echo "tc-local: $*" >&2; exit 1; }
port() { cat "$RUN/port" 2>/dev/null || true; }
pid() { cat "$RUN/server.pid" 2>/dev/null || true; }
# our server only: the pid is alive and runs server.js from our copy
running() {
	local p; p="$(pid)"
	[[ -n "$p" ]] && [[ -r "/proc/$p/cmdline" ]] && tr '\0' ' ' < "/proc/$p/cmdline" | grep -q -- "$RUN/ref/server.js"
}
free_port() { node -e "const s=require('net').createServer();s.listen(0,'127.0.0.1',()=>{console.log(s.address().port);s.close()})"; }
url() { echo "ws://127.0.0.1:$(port)/ws"; }

[[ "$ROOM" =~ ^[A-Za-z0-9]{3,12}$ ]] || die "TC_ROOM must be 3 to 12 letters or digits"

prepare() {
	command -v node >/dev/null || die "node is not installed"
	[[ -d "$REF/.git" || -f "$REF/.git" ]] || die "reference/ww2-rts is missing (git submodule update --init)"
	local commit; commit="$(git -C "$REF" rev-parse HEAD)"
	if [[ "$(cat "$RUN/ref.commit" 2>/dev/null)" == "$commit" && -d "$RUN/ref/node_modules/ws" ]]; then return; fi
	echo "tc-local: copying reference $commit to $RUN/ref"
	rm -rf "$RUN/ref"; mkdir -p "$RUN/ref" "$RUN/saves"
	git -C "$REF" archive HEAD | tar -x -C "$RUN/ref"
	(cd "$RUN/ref" && npm ci --ignore-scripts --no-audit --no-fund >"$RUN/npm.log" 2>&1) || die "npm ci failed, see $RUN/npm.log"
	echo "$commit" >"$RUN/ref.commit"
}

serve() {
	mkdir -p "$RUN"
	if running; then echo "tc-local: already running on $(url) (pid $(pid))"; return; fi
	prepare
	local p="${TC_PORT:-$(free_port)}"
	echo "$p" >"$RUN/port"
	# PUBLIC_URL set: the server then never asks tailscale for this machine's name. SAVES_DIR: saves stay in the copy.
	# exec all the way down (subshell, setsid, nohup, node): the pid recorded is the server's own
	( cd "$RUN/ref" || exit 1
	  export PORT="$p" HOST=127.0.0.1 PUBLIC_URL="http://127.0.0.1:$p" SAVES_DIR="$RUN/saves"
	  exec setsid nohup node "$RUN/ref/server.js" >>"$RUN/server.log" 2>&1 </dev/null ) &
	echo $! >"$RUN/server.pid"
	for _ in $(seq 1 50); do
		if curl -fsS -o /dev/null "http://127.0.0.1:$p/" 2>/dev/null; then echo "tc-local: server up on $(url) (pid $(pid), log $RUN/server.log)"; return; fi
		running || die "server exited, see $RUN/server.log"
		sleep 0.2
	done
	die "server did not answer on port $p within 10 s, see $RUN/server.log"
}

health() {
	running || die "no server started by this script is running (tc-local.sh serve)"
	local p; p="$(port)"
	local code; code="$(curl -sS -o /dev/null -w '%{http_code}' "http://127.0.0.1:$p/")" || die "HTTP request failed"
	echo "http: GET / -> $code"
	[[ "$code" == 200 ]] || die "HTTP status $code"
	node "$HERE/tc-ws-probe.mjs" "$RUN/ref" "$(url)" health
	echo "tc-local: healthy: $(url), pid $(pid), reference $(cat "$RUN/ref.commit")"
}

client() { # extra client arguments follow; the log goes to $RUN/client-<label>.log
	[[ -x "$EDITOR" ]] || die "no editor at $EDITOR (set UE)"
	[[ -f "$PROJECT_DIR/Binaries/Linux/libUnrealEditor-ThreeCrossroads.so" ]] || die "build the editor target first (README)"
	running || die "start the server first: tc-local.sh serve"
	local label="$1"; shift
	"$EDITOR" "$UPROJECT" -game -TCServer="$(url)" -TCRoom="$ROOM" -TCName="$NAME" -abslog="$RUN/client-$label.log" "$@"
}

case "${1:-}" in
	serve) serve ;;
	health) health ;;
	status) if running; then echo "running: $(url) pid $(pid) log $RUN/server.log"; else echo "not running"; fi ;;
	play) client "${TC_SEAT:-host}" -windowed -ResX=1600 -ResY=900 -TCSeat="${TC_SEAT:-host}" -TCHostStart="$AI" ;;
	host) client "${TC_SEAT:-host}" -windowed -ResX=1600 -ResY=900 -TCSeat="${TC_SEAT:-host}" ;;
	join) client "${TC_SEAT:-guest}" -windowed -ResX=1280 -ResY=720 -TCSeat="${TC_SEAT:-guest}" ;;
	watch) client watch -windowed -ResX=1280 -ResY=720 -TCSpectate ;;
	check)
		label=check; ROOM="check$((RANDOM % 100000))" # its own room, never the one being played
		client "$label" -nullrhi -nosound -unattended -nosplash -TCSeat="$label" -TCHostStart=1 -TCAutoMove -TCExitAfter="${TC_CHECK_SECONDS:-45}" >/dev/null 2>&1 || true
		log="$RUN/client-$label.log"
		{ grep -E "LogTC: .*(start #|auto move: |auto move \+9s|not connecting|server refused)" "$log"; grep "LogTC: status:" "$log" | tail -n 1; } | sed 's/^\[[^]]*\]\[[^]]*\]//'
		grep -q "auto move: .* [1-9][0-9]* ordered" "$log" && grep -q "auto move +9s" "$log" || die "check failed, see $log"
		echo "tc-local: check passed (log $log)" ;;
	stop)
		if running; then kill "$(pid)"; for _ in $(seq 1 25); do running || break; sleep 0.2; done; echo "tc-local: stopped"; else echo "tc-local: not running"; fi
		rm -f "$RUN/server.pid" ;;
	*) sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
