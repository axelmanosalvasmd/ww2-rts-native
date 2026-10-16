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
stream. OPERATIVES-2 does not infer complete NET or IF coverage from these
narrower FPS checks.

## OPERATIVES-3 verification scope

This follow-up adds server tests over PR #371. The required merge of main
also composes operative admission with save/load, retains operative membership
when a loaded room replaces its seats, and adds serialization derives required
by the combined core. It proves only the operative portions of these five rows:

- NET-25: `net25_operative_live_takeover_preserves_person_and_ignores_old_socket_close`
  covers a live replacement in lobby and play, token precedence over the requested
  role and commander, the exact replaced message before close, retained person
  state, input reset, and silence from late messages and a late close on the old
  socket (`server.js:596-603,614,620,719`).
- NET-28: `net28_net29_operative_broadcasts_keep_change_order_and_shifted_commander_identity`
  checks actual broadcasts to both seats and both operatives in change order.
  Each roster entry has exactly `name`, `commander`, `online`. Operative lobby
  fields remain `role:"operative"`, `you:-1`, `spectator:false`, `amHost:false`.
  Removing a non-final seat moves the commander's index; removing that commander
  leaves -1 even after another human takes the index (`server.js:338-350`).
- NET-29: the same broadcast test checks cleaning, the 16-character limit,
  empty-name fallback and silence for non-string names. The existing
  `operative_resync_and_rename_cover_lobby_play_pause_and_ending` also covers
  rename broadcasts during play, pause, ending and the returned lobby
  (`server.js:194,635,638`).
- NET-46: `operative_permission_whitelist_is_silent_except_name_and_latency_ping`
  checks valid and malformed army commands, host actions, save/load requests,
  seat changes and map pings. They produce no messages, queued commands, roster
  changes or deny-budget consumption (`server.js:630-636`).
- NET-82: `net82_operative_offline_identity_survives_lobby_cleanup_deadline`
  checks offline roster publication, retained membership past ten seconds and
  token reuse. The live disconnect test also checks cleared input, an idle soldier,
  no pause and no offline respawn. The live takeover test checks that closing a
  replaced socket leaves its successor online (`server.js:568-575,718-725`).

`operative_room_members_deliver_every_tick_through_natural_finish_lobby_and_restart`
adds a room-level lifecycle check. An ordinary Conquest objective earns VP to
finish the match. Both seat snapshots and both independent operative views are
collected on every advancing tick; full seat reads preserve the production
terrain outboxes. Actual scheduled socket delivery retains equal cadence for
every online recipient. One operative disconnects and reconnects during play.
Both retain their roles through the full ending hold and lobby return, then
receive fresh starts on a new match and restart (`server.js:307-319,392-413,560-565,718-725,738-760`).

Spectator portions of NET-25/28/29/46/82 remain with SPECTATORS. Save listings and
loaded starts remain with save/load. FPS-39, NET-26, NET-40, NET-53 and IF-28 are
not claimed by this batch. No World, wire, ABI, header or privacy rule is added.
In particular, the confirmed exact attacker position through age 10 and the
single cached-path nonowner mask at `shared/sim.js:5903` remain unchanged.

The save/load merge retains operatives transactionally: after all restoration
checks pass, each record moves to the loaded room and its commander follows
the same present person into the restored seat array. If that person receives
no saved seat, the commander becomes -1. Failed loads leave the existing room
membership untouched. Operative messages reach their whitelist before the
save/load host handlers, so save, load and loaded-resume requests stay silent
(`server.js:436-450,630-636`).
`operative_load_merge_retains_membership_and_remaps_the_commander_person`
checks a real save and load with reordered present seats, fresh starts and
sequence resets for two operatives, host-only loaded resume and subsequent
fault-free ticks. This is an integration check, not a claim for the full save/load
or FPS-39 row.

The default-map load check also exercises authored houses. Their existing
`WorldStructure.kind` value is `"house"` (`structure::StructureKind::House`),
so the compiled checkpoint-string decoder accepts that tag alongside
`"building"` and `"bridge"`. This restores existing map content without adding
fields or accepting arbitrary static strings.
