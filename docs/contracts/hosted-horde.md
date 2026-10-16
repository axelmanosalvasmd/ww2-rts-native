# Hosted Horde and delivered AI scheduling

Baseline: prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
Owner decisions K and L open hosted Horde and recipient-only room AI. The
existing JSON WebSocket v0 message shapes below are identical to the prototype.
World admission and the C interface remain unchanged. Hermes' exact-head runtime
acknowledgement remains required before merge.

`server.js:649-654,662-675` keeps an eligible current map, otherwise selects the
first eligible map in sorted catalogue order, ignores ineligible selections, and
changes Endless to Standard. `start` and `restart` use a freshly read authored
map and refuse without replacing the previous lobby or runtime when its
`defend` list is gone (`server.js:365-376`). The native immutable embedded source
is the default. Hosts can set `WW2_AUTHORED_MAPS_DIR` to read the exact catalogue
key plus `.json` from a private directory on every Horde start/restart. Missing
or invalid files refuse without falling back to the embedded map. A failed map
read/parse or construction sends every commander
`{t:"deny",cmd:"start",reason:"scenario"}`, including for Restart; losing only
`defend` restores the previous room without that denial (`server.js:365-386`). The same
validated map constructs the match and its startup binding.

The host's `{t:"nextwave"}` has no response payload. While playing unpaused
Horde, it changes only an inactive break clock to zero; normal simulation starts
the wave next tick (`server.js:698-699`). Guest, paused and active-wave requests
do nothing. Host `{t:"pause"}` and `{t:"resume"}` reuse the existing
`{t:"pause",paused,by,reason,left}` shape (`server.js:481-491,700-703`).

Start arrays `names`, `spawns`, `teams`, and `factions` include the internal
Horde seat (`server.js:461-466`, `sim.js:690-693`). Defenders use team 0; Horde
uses team 1, name `Horde`, and the first unused faction from `[1,2,0]`, or 1.
The lobby roster and snapshot `online`/`ping` contain only room commanders
(`server.js:504`; `server/room-tick.js:27`). No socket or lobby Person exists for
the internal seat. Results include it in `names`, `teams`, and `story`, use
`mode:"horde"`, `winVp:null`, and territory timeline entries
`{t,points,structures}` (`server.js:753`; `shared/story.js:52-69`). Existing
`reason`, `at`, `endTick`, and per-recipient `you` meanings are unchanged.

After each simulation step, the server passes its actual adaptive delivery beat
to the shared AI scheduler (`server/room-tick.js:12-21`). All requested recipient
views refresh before brains act. Room AI uses the human commander; the internal
Horde seat uses the scripted director and observes every beat
(`shared/ai-schedule.js:23-40`). Between beats they retain their prior detached
packet. The initial packet comes from the ordinary full recipient producer,
matching `server.js:401-404`. Reconnect/catch-up/pause frames do not refresh AI
views. The director drains at most four pending ground orders per observation
and uses the normal seat/difficulty think stagger (`shared/ai.js:169-210`). Live
wave and winner guards prevent stale orders (`shared/ai.js:248-251`). All output
enters the canonical recorded command queue for the next tick.

AI packets contain only the seat's decoded recipient snapshot, public starting
terrain and public rules. Remembered terrain updates only from delivered cells.
Owner-only plans, wallets and cooldowns are not taken from another seat. No
policy receives a Game or Match authority handle. Existing actor, retained
projectile, full-tick and recipient guards remain in force, including the
restriction on aircraft weapons whose effects are not yet deliverable.

Horde record persistence and `result.horde`/`hordeBest` remain the separate
MODE-55 work. This batch delivers the ordinary story/result fields and does not
claim the record extension. Spectator and save admission retain their existing
separate guards.
