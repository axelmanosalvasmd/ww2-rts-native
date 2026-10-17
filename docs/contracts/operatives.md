# Operatives on JSON WebSocket v0

Baseline: prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
This implementation follows the prototype's server shapes and recipient rules.
Group G is authorized by the stream brief and Julio's recorded owner decision
[#5 comment 6217590741](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6217590741).
Julio confirmed the relayed approval in
[#5 comment 6217727398](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6217727398).
Axel confirmed the operative and privacy decisions in
[#5 comment 6218552386](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6218552386).
Hermes' exact-head runtime review remains required. This document adds no Unreal
interface or World admission.

## Membership and input

`hello` uses the existing `token` and `name`, plus `role: "operative"` and
integer `commander`. A new operative needs a human commander seat, a nonempty
40-UTF-16-code-unit token, fewer than four room operatives, and a non-World room.
Failure sends exactly `{"t":"deny","cmd":"join","reason":"commander"}`
and closes with code 1008. Token reuse precedes role admission, replaces the
socket, sends `{"t":"replaced"}` to the old socket and retains the operative's
commander and soldier (`server.js:596-607`).

Operatives hold no army seat, never become host or spectator, and remain room
members when offline or after a match. Everyone receives the existing lobby
fields plus `operatives: [{name, commander, online}]`. For an operative the lobby
has `role: "operative"`, `commander` equal to the current commander seat or -1,
`spectator: false`, `amHost: false`, and `you: -1`. Commander removal retains the
original person association rather than attaching to a new occupant
(`server.js:205-206,342-350,723`). The existing native `proto: 0` addition remains.

Only `fps`, `resync`, string `name`, and latency `ping` are handled. Army orders,
map pings, host actions and seat changes are silently ignored
(`server.js:620-637`; `server/map-pings.js:4`). An unpaused playing room accepts
at most one `fps` message per operative per 20 ms on the server clock. The
accepted attempt consumes the interval even if simulation input validation
rejects its sequence or fields. Each start resets the timestamp to zero, and
the comparison is `now - inputAt >= 20` (`server.js:312-313,630-635`). The existing
core validates sequence, finite movement and aim values, buttons and edge counters
against `shared/sim.js:2021-2037`.

## Start, snapshot and status

Operative `start` has exactly these keys: `t`, `role`, `operative`, `matchId`,
`map`, `you`, `spawn`, `spawns`, `cells`, `fog`, `names`, `teams`, `factions`,
`weather`. Here `role` is `"operative"`, `you` is the commander seat, `cells`
and `fog` are full, and `operative` is the status below. A current pause message
follows the start. Reconnect resets sequence to -1, input, edge counters and
delivery state while retaining the soldier (`server.js:307-319`). Every match
start recreates the operative start before the room can send snapshots
(`server.js:392-413`).

Each operative has independent terrain, fog, unit-delta and list receipts,
projected through its commander's recipient view. It never receives a full-map
observer overlay. The normal snapshot keys remain, including commander economy,
`online` and `ping`; these ten keys are removed: `movement`, `queues`,
`productionJobs`, `plans`, `orders`, `air`, `rally`, `works`, `covers`, `sup`.
`operative` is added. Full/delta units, `gone`, `held`, `all`, list omission,
terrain and fog keep the existing v0 encodings. Delivery runs on scheduled
snapshot beats, paused frames and the ending hold, independently of commander
socket backpressure (`server.js:285-293,320-329,501-508`;
`server/room-tick.js:28-41`).

Status always contains `id`, `hp`, `maxHp`, `respawn`, `cost`, `squad`,
`squadSpawn`, `waiting`, `seq`, `shots`, `hits`, `kills`. `maxHp` is 20 and
`cost` is 10. `squad` is `{id, name, men}` or null; `men` rounds squad HP up by
HP per man. A living soldier adds `x`, `z`, `vx`, `vz`, `jy`, `crouch`, `weapon`,
`mags`, `reload`, `nades`, `from`. Positions and motion round to 0.01 m,
reload to 0.1 s, HP and remaining respawn seconds round up. `from` is the
attacker's position rounded to metres for ten ticks after the hit, otherwise
null. It follows the prototype even when the attacker is hidden
(`server.js:294-306`). Nonowner unit flags retain the prototype's cached mask,
which excludes the operative flag (`shared/sim.js:5899-5905`), as clarified in
[#5 comment 6217661080](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6217661080).

Operative fire reuses the prototype small-arms event: `k`, `f`, `fo`, `x`,
`y`, `z`, with `t`, `to`, `hit: true` and optional drawn-man index `m` only
when an enemy was hit (`shared/sim.js:2094-2096`). Grenade launch emits `f`,
`fo`, `x`, `z`, `k: "throw"`, `physical: true`, `local: true`
(`shared/sim.js:2098-2105`). The existing shot projection applies recipient
visibility field by field (`shared/sim.js:5998-6008,6011-6060`). A retained operative
grenade uses the prototype `flights` row: `flight`, `kind`, `effect`, `seq`,
`x`, `y`, `z`, `v`, `gravity`, `time`, `launched`, `expires`, `until`, plus
`f` and `fo` only when the shooter is visible. Flight visibility uses its
current ground cell; `until` clips prediction at hidden ground or a visible
physical contact (`shared/sim.js:5966-5996`). Ordinary projectile
admission remains with its separate stream.

Disconnect marks the operative offline and clears input while the battle
continues with its soldier and membership retained. Offline dead operatives
wait for reconnect before respawning. The lobby is rebroadcast after a socket
closes, and empty-room expiry counts connected operatives (`server.js:718-725`; `shared/sim.js:5311-5316`). `resync` marks only
that operative's delta stream full; `name` uses the existing cleaner and
rebroadcasts the lobby (`server.js:634-635`).

This batch enables ordinary native matches. World remains closed. Save/load,
spectator admission and other mode runners remain separate streams.

## OPERATIVES-2 verification scope

The follow-up batch proves FPS-05, FPS-28, FPS-29, FPS-30 and FPS-40 against
the same shapes above. It changes tests and documentation, building on PR #367's
runtime. The server start test checks publication before snapshots, a paused
reconnect's start-before-pause order, all input and edge resets, and suppression
without a commander or in World. The latter only exercises the guard; it does
not admit a running World room (`server.js:307-319,392-413,614`).

The independent delivery test consumes a changed fog mask in the commander
stream and then in each of two operative streams. Each operative receives the
same delta once, after the other recipients have acknowledged it
(`server.js:285-293`). The ordinary-match test collects commander and operative
snapshots and checks status on every tick through death, respawn, natural
Conquest victory and eight subsequent ticks. Literal status assertions cover
every living and dead field, all squad-spawn gates, and attacker ages 0 through
11, including the age-10 boundary (`server.js:294-306`).

Disconnect coverage keeps the soldier alive and idle for several ticks, then
runs beyond its respawn deadline while offline with enough MP to respawn.
Commander snapshots continue without pause or fault. Reconnect reuses the room
record, resets sequence validation and permits respawn again
(`server.js:596-603,718-725`; `shared/sim.js:5311-5316`). Resync coverage includes
play, pause, the ending hold and the returned lobby; only that operative's
delta record becomes full. Before its first start, resync does nothing because
the stream does not exist. String names are cleaned and broadcast to all room
members; non-string names are ignored (`server.js:194,634-635`).

FPS-39 remains unclaimed because its load branch belongs to the save/load
stream. The remaining assigned NET and IF rows stay for a later batch; this
batch does not infer their complete coverage from these narrower FPS checks.
