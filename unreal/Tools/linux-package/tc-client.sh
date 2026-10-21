#!/usr/bin/env bash
# Starts the packaged Three Crossroads Linux client (Development) against a game server.
#
# The client is not a game on its own: it needs a running server. Until the Rust ww2-server exists, that is the pinned
# JavaScript reference server (reference/ww2-rts at the commit in BUILD-INFO.txt), run by
# `unreal/Tools/tc-local.sh serve` from a ww2-rts-native checkout. This package does not contain Node or that server.
#
#   ./tc-client.sh --server ws://127.0.0.1:PORT/ws [--room NAME] [--name NAME] [--seat LABEL | --spectate]
#                  [--user-dir DIR] [--check] [--no-server-check] [-- extra client arguments]
#
#   --server URL     the server's WebSocket URL (or TC_SERVER). Required: there is no public server.
#   --room NAME      room, 3 to 12 letters or digits (or TC_ROOM; default native1)
#   --name NAME      player name (or TC_NAME; default Commander)
#   --seat LABEL     seat label: one seat token per server, room and label, kept owner-only in the user folder
#                    (or TC_SEAT; default player). Two clients on one machine need two labels.
#   --spectate       watch with a throwaway token
#   --user-dir DIR   where settings, seat tokens, logs and screenshots go (or TC_USER_DIR;
#                    default ${XDG_DATA_HOME:-~/.local/share}/three-crossroads-native)
#   --check          only check this machine and the server, start nothing
#   --no-server-check  start without asking the server first
#   after --: passed to the client as is (for example -windowed -ResX=1600 -ResY=900, -RenderOffscreen, -nullrhi,
#             or the unattended -TC switches of the project README)
#
# The seat token never goes on the command line (the engine logs it and ps shows it): -TCToken= is refused here and by
# the client. Use --seat, or -TCTokenFile=<absolute path> after --, or the TC_TOKEN environment variable.
set -euo pipefail

HERE="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)"
ROOT="$HERE/Linux"
BIN="$ROOT/ThreeCrossroads/Binaries/Linux/ThreeCrossroads"

die() { echo "tc-client: $*" >&2; exit 1; }
say() { echo "tc-client: $*"; }

SERVER="${TC_SERVER:-}"; ROOM="${TC_ROOM:-native1}"; NAME="${TC_NAME:-Commander}"; SEAT="${TC_SEAT:-player}"
USER_DIR="${TC_USER_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/three-crossroads-native}"
SPECTATE=0; CHECK_ONLY=0; SERVER_CHECK=1; EXTRA=()
while [[ $# -gt 0 ]]; do
	case "$1" in
		--server) SERVER="${2:-}"; shift 2 ;;
		--server=*) SERVER="${1#*=}"; shift ;;
		--room) ROOM="${2:-}"; shift 2 ;;
		--room=*) ROOM="${1#*=}"; shift ;;
		--name) NAME="${2:-}"; shift 2 ;;
		--name=*) NAME="${1#*=}"; shift ;;
		--seat) SEAT="${2:-}"; shift 2 ;;
		--seat=*) SEAT="${1#*=}"; shift ;;
		--spectate) SPECTATE=1; shift ;;
		--user-dir) USER_DIR="${2:-}"; shift 2 ;;
		--user-dir=*) USER_DIR="${1#*=}"; shift ;;
		--check) CHECK_ONLY=1; shift ;;
		--no-server-check) SERVER_CHECK=0; shift ;;
		-h|--help) sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
		--) shift; EXTRA=("$@"); break ;;
		*) die "unknown option '$1' (client arguments go after --; see --help)" ;;
	esac
done

for A in "${EXTRA[@]+"${EXTRA[@]}"}"; do
	case "$A" in
		-TCToken=*|-tctoken=*) die "-TCToken= is refused: the engine logs its command line and ps shows it to every local user. Treat that token as exposed. Use --seat, -TCTokenFile=<absolute path> or TC_TOKEN." ;;
	esac
done

# This machine
[[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]] || die "this package is for Linux x86_64, not $(uname -s) $(uname -m)"
[[ -f "$BIN" ]] || die "the client binary is missing: $BIN (an incomplete copy of the package?)"
[[ -x "$BIN" ]] || chmod +x "$BIN" 2>/dev/null || die "$BIN is not executable and cannot be made so"
if command -v ldd >/dev/null; then
	MISSING="$(ldd "$BIN" 2>/dev/null | awk '/not found/ {print $1}' | sort -u | tr '\n' ' ')"
	[[ -z "$MISSING" ]] || die "system libraries the client needs are missing: $MISSING(install them with your distribution's package manager)"
else
	say "warning: ldd not found, shared libraries not checked"
fi
RENDER=window
for A in "${EXTRA[@]+"${EXTRA[@]}"}"; do
	case "${A,,}" in -nullrhi) RENDER=none ;; -renderoffscreen) [[ "$RENDER" == none ]] || RENDER=offscreen ;; esac
done
if [[ "$RENDER" != none ]]; then
	# Unreal on Linux renders with Vulkan, opened at run time (ldd above cannot see it): the system loader
	# libvulkan.so.1, else the loader the package carries in Engine/Binaries/ThirdParty/Vulkan/Linux. Either way a
	# driver (an ICD manifest) for the GPU must be installed.
	LOADERS="$( { /sbin/ldconfig -p 2>/dev/null || ldconfig -p 2>/dev/null || true; } )"
	if ! grep -q 'libvulkan\.so\.1 ' <<<"$LOADERS" && [[ ! -f "$ROOT/Engine/Binaries/ThirdParty/Vulkan/Linux/libvulkan.so" ]]; then
		die "no Vulkan loader: libvulkan.so.1 is not installed and the package's own loader is missing. Install your distribution's Vulkan loader and a Vulkan driver, or pass -- -nullrhi for a client that draws nothing"
	fi
	ICDS=0
	if [[ -n "${VK_ICD_FILENAMES:-}${VK_DRIVER_FILES:-}" ]]; then ICDS=1; fi
	for D in /usr/share/vulkan/icd.d /usr/local/share/vulkan/icd.d /etc/vulkan/icd.d "${XDG_DATA_HOME:-$HOME/.local/share}/vulkan/icd.d"; do
		compgen -G "$D/*.json" >/dev/null && ICDS=1
	done
	[[ "$ICDS" == 1 ]] || die "no Vulkan driver found (no ICD manifest in /usr/share/vulkan/icd.d or /etc/vulkan/icd.d): install the Vulkan driver for your GPU (Mesa's vulkan driver or the vendor's), or pass -- -nullrhi for a client that draws nothing"
	if [[ "$RENDER" == window && -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]]; then
		die "no display (DISPLAY and WAYLAND_DISPLAY are unset): run it in a desktop session, or pass -- -RenderOffscreen (no window) or -- -nullrhi (no drawing)"
	fi
fi

# The server
[[ -n "$SERVER" ]] || die "no server: pass --server ws://HOST:PORT/ws (or TC_SERVER). The client needs the temporary reference server: in a ww2-rts-native checkout run unreal/Tools/tc-local.sh serve, then tc-local.sh status shows its URL. The Rust ww2-server is not built yet."
[[ "$SERVER" =~ ^(ws|wss)://([^/:]+|\[[0-9a-fA-F:]+\])(:([0-9]+))?(/.*)?$ ]] || die "--server must be a ws:// or wss:// URL, got '$SERVER'"
SCHEME="${BASH_REMATCH[1]}"; HOSTPART="${BASH_REMATCH[2]}"; PORT="${BASH_REMATCH[4]}"
[[ "$ROOM" =~ ^[A-Za-z0-9]{3,12}$ ]] || die "--room must be 3 to 12 letters or digits"
[[ "$SPECTATE" == 1 || "$SEAT" =~ ^[A-Za-z0-9_-]{1,32}$ ]] || die "--seat must be 1 to 32 letters, digits, - or _"
if [[ "$SERVER_CHECK" == 1 ]]; then
	HTTP="http"; [[ "$SCHEME" == wss ]] && HTTP="https"
	[[ -n "$PORT" ]] || { [[ "$SCHEME" == wss ]] && PORT=443 || PORT=80; }
	if command -v curl >/dev/null; then
		CODE="$(curl -s -o /dev/null -m 5 -w '%{http_code}' "$HTTP://$HOSTPART:$PORT/" 2>/dev/null || true)"
		[[ "$CODE" =~ ^[23] ]] || die "no game server answers at $HTTP://$HOSTPART:$PORT/ (HTTP status '${CODE:-none}'). Start the reference server (unreal/Tools/tc-local.sh serve in a ww2-rts-native checkout) and pass its URL, or --no-server-check."
		say "server answers at $HTTP://$HOSTPART:$PORT/ (HTTP $CODE)"
	else
		say "warning: curl not found, server not checked before starting"
	fi
fi

if [[ "$CHECK_ONLY" == 1 ]]; then say "ready: $BIN, rendering: $RENDER, server $SERVER, user folder $USER_DIR"; exit 0; fi

mkdir -p "$USER_DIR" && chmod 700 "$USER_DIR" || die "cannot create the user folder $USER_DIR"
ARGS=(ThreeCrossroads -UserDir="$USER_DIR/" -TCServer="$SERVER" -TCRoom="$ROOM" -TCName="$NAME")
if [[ "$SPECTATE" == 1 ]]; then ARGS+=(-TCSpectate); else ARGS+=(-TCSeat="$SEAT"); fi
exec "$BIN" "${ARGS[@]}" "${EXTRA[@]+"${EXTRA[@]}"}"
