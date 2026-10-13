# WebSocket protocol at the pin

This documents the protocol as implemented at `reference/ww2-rts` commit e4e579a. It is an inventory, not a contract: the agreed protocol will live in `docs/contracts/protocol.md`, which cites this file. Every `path:line` is relative to `reference/ww2-rts`. Anything not pinned down in code is marked **unverified**. Behavior rows that the Rust server and Unreal client must reproduce are in [`server.md`](server.md) (NET-xx) and [`fps.md`](fps.md) (FPS-xx).

Counts: 60 client-to-server `t` values (29 session and room messages, 31 gameplay commands passed to `command()`), 11 server-to-client `t` values. Three more messages travel between processes in shard mode (section 7); they never reach a client.

## 1. Transport

| Item | Value | Evidence |
|---|---|---|
| URL | `ws://<host>/ws?room=<code>`, `wss://` when the page is HTTPS. The browser builds it from `location`. | client/main.js:168-169 |
| Path | `/ws` only. Non-shard: the `ws` server is created with `path: '/ws'` (other paths are rejected by the library, status **unverified**). Shard front: other upgrade paths are destroyed. | server.js:578; server/shards.js:37-38 |
| Query parameters | `room` only, required, `/^[a-z0-9]{3,12}$/i`; otherwise the socket is closed with 1008 "bad room". Case is kept: `ABC` and `abc` are different rooms. | server.js:580-581 |
| Subprotocol | None requested by the browser; the server sets no `handleProtocols` (behavior if a client offers one: **unverified**). | client/main.js:168-171; server.js:578 |
| Compression | permessage-deflate, `threshold: 1024` (messages under 1 KiB go uncompressed). Other deflate parameters are `ws` 8.22.0 defaults (**unverified** in detail). | server.js:577-578; node_modules/ws 8.22.0 |
| Max inbound payload | 16,384 bytes per message. Overflow closes the socket (`ws` uses close code 1009, **unverified** here). | server.js:578 |
| Encoding | UTF-8 JSON text, one object per frame, discriminated by string `t`. Unparsable frames and non-objects are ignored. Binary frames are not used (a binary frame is passed to `JSON.parse` the same way, **unverified** in practice). | server.js:584-587; client/connection.js:36-39 |
| Keepalive | No WebSocket ping frames from the server and no server idle timeout. The client sends an application `ping` every 2 s and expects `pong`; three seconds without a pong counts as a loss for the stats overlay only. | client/main.js:172; client/perf.js:16-23 |
| Origin and auth | No origin check (`verifyClient` unset). No authentication: the client-chosen `token` is the only identity. | server.js:578, 595-596 |
| Close codes sent | 1008 "bad room" (invalid code); 1008 "operative needs human commander; world mode unsupported"; 1013 "too far behind" (spectator over 4 MiB buffered); plain `close()` after `full`, `replaced` and `left`. | server.js:581, 606, 271, 591, 602, 611, 711 |
| Shard mode | With `WW2_SHARDS=N` the front replays the upgrade request to game process FNV-1a(code) mod N and pipes the sockets; the protocol is unchanged. | server/shards.js:8-13, 35-52 |

## 2. Conventions

- **Seat (`slot`, `you`)**: index into the room's `players`. Spectators and operatives have no seat (`you: -1` in their lobby; an operative's `start.you` is its commander's seat; a spectator's `start.you` is 0).
- **Positions** are metres on the ground plane (`x`, `z`), usually rounded to 0.1. A cell is 2 m (`CELL`); cell index `c = y * w + x`.
- **Time**: the simulation runs at 20 ticks per second (`TICK = 0.05`, shared/sim.js:27). Durations sent to clients are seconds unless named `tick`.
- **Unit row**: the array described in section 5.2. Field positions are named by `UNIT_ROW` (shared/sim.js:5887).
- **Absent fields** in a snapshot mean "not applicable" (undefined is dropped by `JSON.stringify`), except `wrecks` and `nodes`, where absent means "unchanged since the last snapshot" (section 5.4).

## 3. Client to server

### 3.1 Session and room messages (29)

All are handled in `server.js` inside `wss.on('connection')`. "Host" means `isHost` (NET-30). Unless stated, the only reply is a lobby rebroadcast to every seat, spectator and operative.

| `t` | Fields | Who and when | Handler | Effect and replies |
|---|---|---|---|---|
| `hello` | `token: string` (first 40 chars used), `name: string`, `role?: 'operative'`, `commander?: integer seat`, `spectate?: true`, `listing?: {public?: boolean, title?: string, tutorial?: boolean}` | First message on a socket. Anything sent before it is ignored. A second hello is not special-cased (during play it reaches `command()` and yields `deny blocked`). | server.js:588-617 | Creates the room if new (`listing` applies only then), rejoins by token, or adds an operative, seat or spectator. Replies: `lobby` (always, to everyone), then `start` (and `pause`) if in play; or `full` + close; or `deny {cmd:'join',reason:'commander'}` + close 1008 for a refused operative. The browser always sends `role`, `commander` (0-5) and `spectate` (client/main.js:170). |
| `ping` (latency) | `c: any` (echoed), `rtt?: number`, `d?: number[12]` (browser stats, order `CLIENT_KEYS`) | Anyone, any state. Must not contain `x` or `z` keys. | server.js:621-629 | Stores `rtt` clamped 0-9999 (shown in snapshots' `ping`); with `WW2_DIAG=1` stores the clamped `d`. Reply `pong` to the sender only. |
| `ping` (map) | `x: number`, `z: number` | Seated human in play. | server.js:622; server/map-pings.js:3-15 | Finite and inside the map, at most 3 per 5 s per player (invalid ones do not count). Relayed `ping {from, x, z}` to connected human seats on the sender's team, sender included. Otherwise dropped silently. |
| `name` | `name: string` | Anyone after hello (operatives too). | server.js:635, 638 | Cleaned (strip `<>&"'`, trim, 16 chars, default "Soldier"). |
| `resync` | none | Seat or spectator in play; operative in any state. | server.js:634, 684-687 | Marks the sender's delta record full (outside World a spectator marks the shared spectator stream). The next `s` to it has `all: true`. No direct reply. |
| `fps` | `seq: safe integer`, `forward`, `strafe` in [-1, 1], `yaw` (abs at most 1e4), `pitch` (abs at most 1.45), `fire?`, `ads?`, `crouch?`, `sprint?` (each counts only if exactly `true`), `weapon?: 0 or 1`, `jump?`, `reload?`, `nade?`, `squad?` (non-negative running counts) | Operative, match in play, not paused; at most one per 20 ms per operative. | server.js:631-633; shared/sim.js:2018-2040 | Sets the operative's intent; rejected input (stale `seq`, out of bounds) is ignored. No reply. Built by client/operative-controls.js:11-13 and client/operative-view.js:70-74. |
| `addAi` | none | Host, lobby, room under 6 seats and (unlisted or under map seats). | server.js:639-641 | Adds seat "AI n", level `normal`. |
| `level` | `slot: integer`, `v: 'easy' or 'normal' or 'hard'` | Host, lobby, slot is an AI. | server.js:642-643 | Sets that AI's difficulty. |
| `kick` | `slot: integer >= 0` | Host, lobby, target is AI or an offline human. | server.js:644-646 | Removes the seat; result lists realign. |
| `team` | `slot: integer >= 0`, `v: integer 0-5` | Not in play. Host for any seat. | server.js:655-658 | Sets the seat's team. |
| `faction` | `slot: integer >= 0`, `v: integer 0-3` | Not in play. Host for AI seats and itself; anyone for its own seat. | server.js:655-658 | Sets the seat's faction. |
| `map` | `name: string` (must be in the map list) | Host, not in play. | server.js:649-654 | Loads the map (async); ignored if play started, the host changed, the mode is Tutorial, or Horde and the map has no defenders. Sets `mapName`, seat count. |
| `mode` | `v`: `conquest`, `assault`, `annihilation`, `classic`, `horde`, `tutorial`, `world` | Host, not in play. | server.js:659-671 | Tutorial forces map `tutorial`; Horde picks a defender map (async) and turns `endless` army into `standard`; a newer `mode` request supersedes a pending one. |
| `worldSize` | `v` or `size`: `huge` or `massive` | Host, not in play. | server.js:672-673 | The browser sends `{t:'worldSize', size}` (client/main.js:264). |
| `army` | `v`: `standard`, `large`, `massive`, `endless` (not `endless` in Horde) | Host, not in play. | server.js:674-675 | Army size preset. |
| `sandbox` | `v: any` (truthiness) | Host, not in play. | server.js:676-677 | Sandbox flag for the next match. |
| `weather` | `v`: one of `WEATHER_CHOICES` (`map`, each weather kind, `random`) | Host, not in play. | server.js:678-679; shared/weather.js:22 | Weather for the next match. |
| `defender` | `v: integer 0-5` | Host, not in play. | server.js:680-681 | Assault defending team. |
| `spectate` | none | Seated, lobby, fewer than 8 spectators. | server.js:704-705 | Gives up the seat, becomes a spectator. |
| `sit` | none | Spectator, lobby, a seat free under the room's limits. | server.js:706-707 | Takes a seat with new defaults. |
| `start` | none | Host, not in play, 1 to `seats` players, Assault teams valid. | server.js:682-683, 358-390 | Starts a match (section 6.3). On a bad map: `deny {cmd:'start',reason:'scenario'}` to every seat. |
| `restart` | none | Host, in play, game running. | server.js:688-689 | Same as `start`, new `matchId`. |
| `save` | none | Host, in play, game running. | server.js:690-693 | Writes a manual save. Reply to the sender: `saved {id}` (plus lobby rebroadcast) or `deny {cmd:'save',reason:'save'}`. |
| `loadSave` | `id: string` matching `<room>-<13 digits>-(auto or manual)` and starting with this room's code | Host, lobby. | server.js:694-695, 433-452 | Loads the save into the room, which starts paused. Failure: `deny {cmd:'loadSave',reason:'save'}` and a lobby rebroadcast. |
| `end` | none | Host, in play. | server.js:696-697 | Back to the lobby with result `{ended: true}`. |
| `nextwave` | none | Host, Horde, in play, not paused. | server.js:698-699 | Ends the current break (wave starts). No reply. |
| `pause` | none | Host, in play. | server.js:700-701 | Host pause; `pause` broadcast. |
| `resume` | none | Host, in play. | server.js:702-703 | Ends any pause (host or drop); `pause {paused:false}` broadcast. |
| `handAi` | `slot: integer` | Host, in play, target is an offline human. | server.js:647-648 | That seat becomes an AI ("<name> (AI)"). |
| `leave` | none | Seated human, in play. | server.js:708-711 | An AI takes the seat; reply `left` to the sender, then the socket is closed. |

Operatives: only `ping`, `fps`, `resync`, `name` are handled; everything else is dropped (server.js:630-637). Spectators: lobby and match controls work only for the host; every other message is dropped without reply (server.js:712).

### 3.2 Gameplay commands (31)

Any message from a seated player while the room is in play, the game exists and no pause is active, whose `t` is not handled above, is passed whole to `command(g, slot, msg)` (server.js:712-715). A returned reason string is answered with `deny {cmd: t, reason}` to the sender, at most 4 denials per rolling second per player (shared/command-feedback.js:2-8). No reply on success; the effect shows up in later snapshots.

Common rules (shared/sim.js:3408-3420): every command is refused with `blocked` once there is a winner, for a seat that is `out` or `away`, or for a malformed `orders` array (any row not `[safe integer, finite, finite, ...]`). `ids` and `orders` are cut to max(50, unit count). Units must be the sender's own, not an operative, not a structure, not a train, not riding (`mine`, shared/sim.js:3419). Semantics belong to the owning sections (CMB, MOV, BASE, ENG, LOG, MODE); this table fixes the wire shape.

| `t` | Fields | Handler | Refusal reasons seen in code | Section |
|---|---|---|---|---|
| `move` | `orders: [[id, x, z], ...]`, `face?: number`, `queue?: true`, `together?: true` | shared/sim.js:3421-3461 (trucks first in supply-convoys.js:612-640) | blocked, queueFull | MOV |
| `amove` | same as `move` | shared/sim.js:3421-3461 | blocked, queueFull | MOV, CMB |
| `attack` | `ids: id[]`, `target: id`, `queue?` | shared/sim.js:3495-3510 | unseen, needs, queueFull | CMB |
| `fireat` | `ids`, `x`, `z`, `queue?` | shared/sim.js:3462-3477 | blocked, notVisible, needs, queueFull | CMB |
| `garrison` | `ids`, `x`, `z`, `queue?` | shared/sim.js:3478-3494 | blocked, notVisible, needs, retreating, max, queueFull | ENG |
| `stop` | `ids` | shared/sim.js:3511-3513 | needs | CMB |
| `escort` | `ids` (planes), `target` (friendly ground unit) | shared/sim.js:3514-3519 | unseen, needs | MOV |
| `board` | `ids`, `target?` (carrier id) | shared/sim.js:3520-3539 | unseen, needs, retreating, max | MOV |
| `rail` | `ids`, `x`, `z` | shared/sim.js:3540-3563 | blocked (not World), needs, station, cooldown, railCut | MOV |
| `unload` | `ids` (carriers) | shared/sim.js:3564-3570 | needs, shore | MOV |
| `retreat` | `ids` | shared/sim.js:3571-3573 | needs | CMB |
| `stance` | `ids`, `key: 'holdFire' or 'holdPos' or 'autoRetreat'`, `on: boolean` | shared/sim.js:3574-3579, 3333 | blocked, needs | CMB |
| `cover` | `ids`, `queue?` | shared/sim.js:3580-3601 | needs, retreating, noCover, queueFull | CMB |
| `ability` | `ids`, `x?`, `z?`, `queue?` | shared/sim.js:3602-3628 | needs, ammo, cooldown, retreating, mun, blocked, queueFull | CMB |
| `autocast` | `ids`, `on: boolean` | shared/sim.js:3629-3634 | blocked, needs | CMB |
| `dig` | `ids` (first unit digs), `kind?` (a `FORTS` key, default `trench`), `x`, `z`, `dir?`, `queue?` | shared/sim.js:3635-3650 | tier, blocked, noBuilders, max, queueFull, placement reasons (others **unverified**: line 3640 chains more) | ENG |
| `entrench` | `ids`, `pattern` (an `ENTRENCH` key), `x`, `z`, `x2?`, `z2?`, `fort?`, `join?` (project id), `queue?` | shared/sim.js:3651-3679 | blocked, tier, retreating, noBuilders, queueFull, notVisible, mp | ENG |
| `support` | `kind` (a `SUPPORT` key), `x`, `z`, `dir?` | shared/sim.js:3680-3688 | blocked, cooldown, the currency short (`mp`, `mun` or `fuel`), unseen, pop, airfield | MOV (off-map support) |
| `build` | `ids` (builders), `kind`, `x`, `z`, `dir?`, `queue?` | shared/sim.js:3689-3709 | queueFull, retreating, noBuilders, blocked, needs, tier, suddenDeath, placement reasons (line 3695 chains more, **unverified**) | BASE |
| `assist` | `ids`, `id` (building), `queue?` | shared/sim.js:3710-3722 | unseen, retreating, noBuilders, queueFull | BASE |
| `tech` | `kind: 'tier'` or a `TECH.lines` key | shared/sim.js:3723-3736 | blocked, suddenDeath, max, tier, needs, queueFull, mp, mun | BASE |
| `cancelProduction` | `id` (building), `job` (job id) | shared/sim.js:3737-3747 | needs, suddenDeath, unseen, notWaiting | BASE |
| `cancel` | `id` (own unfinished site) | shared/sim.js:3748-3759 | unseen | BASE |
| `destroy` | `ids?`, `id?` | shared/sim.js:3760-3765 | unseen | CMB |
| `rally` | `x`, `z`, and in base modes `id?` or `ids?` (buildings) | shared/sim.js:3766-3780 | blocked, needs, unseen | BASE |
| `buy` | `unit` (type), `from?` (building id) | shared/sim.js:3781-3811 | needs, tier, mp, fuel, pop, max, queueFull, suddenDeath; unknown unit gives blocked | BASE |
| `recover` | none (World only) | shared/sim.js:3415, 1358-1376 | needs, territory, mp, pop, queueFull, blocked | MODE |
| `orders` | `commands: [{t, ...}]` (first 8 used; parts limited to move, amove, attack, garrison, fireat, escort, assist, rally, entrench, board), `queue?: true` applied to every part | shared/sim.js:3395-3406, 3416 | refused only if every part is refused: the first part's reason, else blocked | MOV, CMB |
| `spawn` | `unit`, `x`, `z`, `enemy?: boolean` | shared/sim.js:3417, 4348-4360 | blocked (not sandbox, structure, bad input, 2,000 units) | MODE |
| `supply` | `ids` (trucks), `target` (unit or cache id) | shared/supply-convoys.js:593-611 | needs, blocked; blocked when logistics is off | LOG |
| `logisticsResume` | `ids` (trucks) | shared/supply-convoys.js:578-592 | needs; blocked when logistics is off | LOG |

The browser maps each reason to a sentence (client/availability.js:6-23); `queueFull` on anything but `buy` reads as "This unit already has 8 queued orders". Reasons carry no target details by design (client/availability.js:5).

## 4. Server to client (11)

| `t` | Fields | Sent by | When | Recipients |
|---|---|---|---|---|
| `lobby` | see 4.1 | `lobby()` server.js:338-351 | After every join, leave, setting change, start, end, save; built after an async map listing, so it can arrive after a `start` sent in the same turn. | Every seat, spectator and operative, each with its own `you`, `role`, `amHost`, `result.you`. |
| `start` | see 4.2 | `sendStart` server.js:455-468; `sendOperativeStart` server.js:307-319 | Each match start or load (after the lobby), and to anyone joining during play. | The seat, spectator or operative concerned. |
| `s` | see section 5 | `server/room-tick.js:24-42`; `sendFrame` server.js:502-508; catch-up server.js:769-775 | Every `snapEvery` ticks (2-4) in play, each step of the end hold, once a second while paused, once after a backlog drains. | Each connected seat that is not backed up, spectators, operatives; content per recipient. |
| `pause` | `paused: boolean`, `by: string` (player name, `''` when not paused), `reason: 'host' or 'drop'`, `left: integer seconds` (drop pause countdown, else 0) | `pauseMessage` server.js:470-479 | On pause, resume, every match start (as not paused), every second while paused, and on join during a pause. | Everyone in the room (joiners: just them). |
| `pong` | `c` (echo of the ping's `c`), `srv?: [p95 ms of snapshot ticks (0.1), snapEvery]` (only during play) | server.js:625-628 | Reply to a latency `ping`. | Sender. |
| `ping` | `from: seat`, `x`, `z` (0.1 m) | server/map-pings.js:11-14 | Accepted map ping. | Connected human seats on the sender's team, sender included. |
| `deny` | `cmd: string` (the refused `t`, or `join`, `start`, `save`, `loadSave`), `reason: string` | server.js:369, 385, 606, 692, 695, 714 | Refused command, refused operative join, failed start, save or load. | Sender (start failures: every seat). |
| `saved` | `id: string` | server.js:692 | Manual save written. | Sender. |
| `full` | `reason: 'capacity' or 'started' or 'seats'` | server.js:591, 611 | Room cap reached; no seat or spectator place. Socket closed after. | Sender. |
| `replaced` | none | server.js:602 | Another socket said hello with the same token. Old socket closed after. | The old socket. |
| `left` | none | server.js:711 | After `leave`. Socket closed after. | Sender. |

### 4.1 `lobby` fields

From server.js:341-350.

| Field | Type | Meaning |
|---|---|---|
| `code` | string | Room code. |
| `state` | `'lobby'` or `'play'` | Room state. |
| `you` | integer | Receiver's seat, -1 for spectators and operatives. |
| `host` | integer | Host seat (`hostOf`), -1 when no human holds a seat. |
| `amHost` | boolean | Receiver is host (covers a spectator hosting an all-AI room). |
| `role` | `'commander'` or `'operative'` | Receiver's role (spectators report `commander`). |
| `commander` | integer or absent | Operatives only: commander's seat, -1 if gone. |
| `spectator` | boolean | Receiver has no seat and is not an operative. |
| `operatives` | `{name, commander, online}[]` | Every operative in the room. |
| `spectators` | string[] | Spectator names. |
| `players` | `{name, connected, ai, team, faction, level}[]` | Seats in order; `connected` is true for AI; `level` null for humans. |
| `listed` | boolean | Shown in `/api/rooms`. |
| `maps` | string[] | Map names on disk. |
| `mapName` | string | Current map (`'world'` in World). |
| `spawns` | integer | Seat capacity (`seats(room)`). |
| `mode`, `worldSize`, `defenderTeam`, `army`, `weather`, `sandbox` | settings | Defaults `huge`, `standard`, `map`, false. |
| `hordeMaps` | string[] or absent | Horde mode only: maps with defenders. |
| `hordeBest` | record or null | Horde only: best run for this map, team size and army. |
| `saves` | `{id, kind, savedAt, mode, minutes, names, world?}[]` | This room's saves, newest first. |
| `publicUrl` | string | Invite base (NET-14). |
| `result` | object or null | Last match result: `{ended: true}` when the host ended it, else `{winner, teams, names, reason, at, endTick, story, timeline, winVp, mode, horde?}` (shared/story.js:66-70) plus `you` (`{outcome, team}` for a seat after a decided match, else null). |

### 4.2 `start` variants

| Variant | Fields | Evidence |
|---|---|---|
| Seat | `matchId`, `map` (client map projection, or in World the team's known map from `worldMapFor`), `you`, `spawn`, `spawns` (World: other teams `null`), `cells` (full remembered terrain, 5.6), `fog` (`{v, e}`, 5.7), `names`, `teams`, `factions`, `weather` (`[now]` or `[now, next, seconds]`), `sandbox` | server.js:455-466 |
| Spectator | Seat 0's start built from a projection; outside World `fog` is absent (no mask) and terrain comes from a fresh spectator memory; in World it has fog and seat 0's team map. | server.js:457-466 |
| Operative | `role: 'operative'`, `operative` (status, 5.8), `matchId`, `map`, `you` (commander seat), `spawn`, `spawns` (all), `cells`, `fog`, `names`, `teams`, `factions`, `weather`; no `sandbox`. | server.js:315-317 |

In every variant a `pause` message follows immediately when the room is paused (server.js:318, 467).

## 5. Snapshot (`t: 's'`)

### 5.1 Envelope

Built by `snapshotFor(g, slot, shots, cells, cache)` (shared/sim.js:6011-6090), then `trimmed` (server.js:226-245) adds the delta fields, and the room adds `online` and `ping`.

| Key | Content | Who gets it |
|---|---|---|
| `tick` | Simulation tick. | all |
| `units` | Changed unit rows only (5.2, 5.3). | all |
| `gone` | Ids the receiver should drop (dead or no longer seen); absent when none. | all |
| `all` | `true` when `units` is the complete set (replace, do not merge). | all |
| `held` | `[count, xor of ids]` the receiver should now hold. | all |
| `online` | boolean per seat (connected or AI). | all |
| `ping` | per seat: rtt ms, -1 for AI, null unknown. | all |
| `winner`, `end` | Winning team (-1 draw) or null; `{reason, x, z}` once decided. | all |
| `mode` | Mode row: kind, defenderTeam, attackerTeam, timeLeft, suddenDeath, total, bunkers, profile, nextProfile, wave, left, active, boss, slot, goal, step, steps (shared/sim.js:5895-5898). | all |
| `points`, `vp`, `out` | Per point `[owner, capper, progress, contested, cut, locked]`; VP and `out` per seat. | all (contested per receiver) |
| `weather`, `wx`, `army` | Weather row; `[rain, wetness, wind angle, wind strength]`; army preset `{pop, income}`. | all |
| `nodes` | Resource nodes `[x, z, rate, fuel 0/1]` (World: only on known cells). Absent = unchanged. | all |
| `wrecks` | Remembered wrecks `[id, type, owner, x, z, rot, motion?]`. Absent = unchanged. | all |
| `cells` | Remembered terrain rows changed since the last snapshot (5.6). | all |
| `fog` | Run string of cells that flipped since the receiver's last fog (5.7); absent when unchanged, for non-World spectators and AI. | seats, World spectators, operatives |
| `smokes`, `fires`, `falling` | Visible smoke `[x, z, r, id]`, burning cell indexes, collapsing sections. | all (filtered) |
| `strikes` | `[kind, x, z, dir, seconds, owner]`; World: allied or seen only; elsewhere public. | all |
| `flights`, `shots` | Projectiles and shot events filtered per receiver (shared/sim.js:5985-6009, 6079-6088). Shot keys include `k`, `f`, `fo`, `t`, `to`, `x`, `y`, `z`, `hit`, `kill`, `pub`, `m`, `local`, `text` (CMB owns the list). | all |
| `scenario` | `{version: 1, objectives}` filtered by recipient policy (shared/scenarios.js:171-174). | all (spectators: public objectives only) |
| `ghosts` | Remembered enemy buildings not seen now `[id, type, owner, x, z, built]` (base modes). | seats, operatives |
| `foundMines` | Cell indexes of enemy mines the team has found. | all |
| `world` | World only: `home, size, total, goal, owned, cap, regions, stations, recovery` (shared/sim.js:1450-1471). | seats, operatives (spectators lose `home, owned, cap, recovery`) |
| `home` | World only: `[x, z]` of the team's home. | seats, operatives |
| `mp`, `inc`, `mun`, `fuel`, `fuelInc`, `upkeep` | Receiver's resources. | seats, operatives |
| `tech` | `{tier, armory, lab: [[kind, progress]]}` when tech is on. | seats, spectators (seat 0's), operatives |
| `logistics` | Receiver's logistics block (shared/supply-convoys.js:646). | seats, operatives |
| `movement` | Own units `[id, outcome, tick, ...result]`. | seats |
| `queues`, `productionJobs` | Own production buildings `[id, progress, rallyX, rallyZ, ...types]`; `[id, jobs[]]`. | seats |
| `plans`, `orders` | Own units `[id, kind, x, z, ...path]`; waiting orders `[id, count, kind, x, z, ...]`. | seats |
| `rally`, `air`, `covers`, `works`, `sup` | Own rally `[x, z]` or null; own planes `[id, state, fuel, ammo, rearm]`; team fighter covers; team dig works; support cooldowns `{kind: seconds}`. | seats only (removed for spectators and operatives) |
| `operative` | Status block (5.8). | operatives |

Spectator removals: `movement, queues, productionJobs, plans, orders, air, mp, inc, mun, fuel, fuelInc, upkeep, sup, rally, home, works, covers, logistics` (server.js:251). Operative removals: `movement, queues, productionJobs, plans, orders, air, rally, works, covers, sup` (server.js:326).

### 5.2 Unit row

`[id, type, owner, x, z, rot, aim, hp, supp, target, cover, cd, flags, stars, built, ...tail]` (shared/sim.js:5887-5892).

| Index | Field | Notes |
|---|---|---|
| 0 | `id` | integer |
| 1 | `type` | unit type key (string) |
| 2 | `owner` | seat |
| 3-6 | `x`, `z`, `rot`, `aim` | 0.1 rounding |
| 7 | `hp` | ceil |
| 8 | `supp` | suppression, rounded |
| 9 | `target` | target id, 0 unless the receiver sees the target |
| 10 | `cover` | 0 none, 1 cover, 2 trench, 3 cover on one side |
| 11 | `cd` | ability cooldown seconds, own units only (else 0) |
| 12 | `flags` | bit field below |
| 13 | `stars` | veterancy 0-3 |
| 14 | `built` | 0-1 for buildings, else 1 |
| 15+ | tail | ground vehicles: `[speed, vx, vz, travelDir]`; a digging squad: `[dig progress 0-1]`; else nothing |

Flag bits (shared/sim.js:3334, 5878-5883, 6038-6041): 1 retreating, 2 ability active, 4 AP loaded, 8 reinforcing or healing, 16 digging, 32 garrisoned, 64 attack-moving, 128 building a site, 256 hidden sniper, 512 plane at base, 1024 on a mass entrenchment, 2048 hold fire, 4096 hold position, 8192 auto-retreat, 16384 autocast, 32768 dust, 65536 gate open, 131072 operative, 262144 riding, 524288 carrying squads, bits 20-22 house type of a garrison, bits 23-24 number of squads carried. For rows the receiver does not own, only bits 0-10, 15, 18 and 20-22 survive (shared/sim.js:5903).

### 5.3 Unit deltas and baseline

`unitDelta(sent, rows, full)` (shared/sim.js:5948-5964), one `sent` map per seat (`p.net`), per operative, per World spectator, and one shared map for non-World spectators (`room.watchNet`):

1. A row is sent when `full` is set, the id is new to `sent`, or any field differs from the last sent row.
2. Ids in `sent` but not in this tick's visible rows are listed in `gone` and removed from `sent`.
3. `held = [sent.size, XOR of all ids in sent]`.
4. `all: true` when `full` was set; `full` is then cleared.

Client rule (client/world-state.js:11-27): if `all` is set or `held` is missing, clear held rows; delete `gone` ids; upsert `units`; recompute count and XOR; if they differ from `held`, the client is out of sync and sends `resync` (at most once a second, client/main.js:916-923). It then replaces `s.units` with the full held set and carries over `wrecks` and `nodes` from the last snapshot when absent.

Baseline recovery paths:
- **Start or reconnect**: a seat's `sent` map is replaced (server.js:459), so the next snapshot contains every row (without `all`); the client resets its rows on `start` (client/main.js:506). Operatives get `full: true` (server.js:313). Spectators: World gets `full: true` (server.js:457); non-World marks the shared stream full, so every spectator gets an `all` snapshot (server.js:458).
- **Resync**: `resync` sets `full` (server.js:634, 684-687).
- **Backpressure**: a skipped seat gets one catch-up snapshot after its writes drain; deltas count from the last snapshot actually sent (server.js:762-776).
- **Spectator too far behind**: closed with 1013; it reconnects and starts over (server.js:266-272).

### 5.4 Lists that are only resent on change

`wrecks` and `nodes` (server.js:222-245). Outside World, each list gets a room-level version that moves when the JSON of the cache's list changes; a receiver gets the list when its recorded version differs or on `full`. In World, each receiver's own list JSON is compared with what it last got. Absent means unchanged.

### 5.5 Who gets which stream

| Recipient | Builder | Delta record | Terrain memory | Fog |
|---|---|---|---|---|
| Seat | `snapshotFor(g, i, ...)` | `p.net` per seat | the seat's own (`p.terrainMemory`, `p.terrainPending`) | the seat's fog version |
| AI seat | `observe` path (AI section) | none (no socket) | the seat's own | omitted (`omitFogMask`) |
| Spectator, non-World | `watchGame`: seat 0 with reveal, every unit visible | `room.watchNet` shared; one JSON string for all | `g.watchTerrain` / `g.watchPending` shared | none |
| Spectator, World | `worldWatchGame`: seat 0's team view, no reveal | `s.net` each | copy of seat 0's memory per spectator | its own |
| Operative | `operativeView`: commander's seat with the operative's own memory | `person.net` each | own copy, seeded from the commander's | its own |

Evidence: server.js:201-221, 266-293, 320-329; shared/sim.js:6077.

### 5.6 Terrain cell rows

`[c, ch, level, state, data]` (shared/sim.js:2509-2532, 2654): cell index, terrain character (a hidden enemy mine is reported as the physical cell underneath), height level, cell state, and a data object with `ground`, `object`, `material`, `groundMaterial`, optional `groundWear`, `trenchFront`, `mine`, `mineMaterial`, `mineOwned`, `rail`, `station`, `section` ({id, structureId, state, hp, maxHp, material, anchor}), and in World `initial` ([original char, original height]), `biome`, `landmark`. MAP owns the meaning of each field. `start.cells` holds the receiver's whole memory; `s.cells` only rows changed since its last snapshot.

### 5.7 Fog encoding

`packRuns` and `unpackRuns` (shared/sim.js:4646-4690):

- The map's cells are read in index order as alternating runs, starting with a run of "not set" (full form) or "unchanged" (delta form).
- Each run length is written as base-32 digits, least significant first. Digit values 0-31; values 32-63 mean "this digit plus continue" (bit 5 is the continuation flag).
- Digit alphabet: `ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_` (value = position).
- Full form (`start.fog`): `{v: runs of currently seen cells, e: runs of ever-seen cells}`, both packed against all zeros.
- Delta form (`s.fog`): one string of cells whose seen state flipped since this receiver's last fog version; the client XORs them into its seen mask and marks newly seen cells explored (client/fog.js:71-74). Absent when nothing changed. Teammates share one packed delta per version.

### 5.8 Operative status block

`operative` in an operative's `start` and in each of its snapshots (server.js:294-306): `id` (soldier unit id, 0 when dead), `hp`, `maxHp` (20), `respawn` (seconds), `cost` (10), `squad` (`{id, name, men}` or null), `squadSpawn` (boolean), `waiting` (boolean: no soldier, no squad spawn, commander under 10 MP), `seq` (last accepted input), `shots`, `hits`, `kills`, and while alive `x`, `z`, `vx`, `vz`, `jy` (0.01 rounding), `crouch`, `weapon`, `mags` (array per weapon), `reload` (seconds, 0.1), `nades`, `from` (`[x, z]` of the last attacker within 10 ticks, else null).

## 6. Session lifecycle

### 6.1 Connect and join

1. Client opens `/ws?room=<code>`; invalid code: close 1008.
2. Client sends `hello` on open (every reconnect too).
3. Server: room created if new (or `full {reason:'capacity'}` + close); identity resolved by token (rejoin, see 6.6), else operative, seat or spectator, else `full {reason:'started' or 'seats'}` + close.
4. Server broadcasts `lobby` to everyone. The first server message marks the client connected (client/connection.js:48).
5. Tutorial room: the first player's hello starts the match at once.

### 6.2 Lobby

Host sends settings (`map`, `mode`, `worldSize`, `army`, `sandbox`, `weather`, `defender`, `team`, `faction`, `level`, `addAi`, `kick`); anyone sends `name`, own `faction`, `spectate`, `sit`. Each change: `lobby` to everyone. Host `loadSave` jumps to 6.3 with a saved game.

### 6.3 Start

1. Host `start` (or `restart`, `loadSave`).
2. Room state becomes `play` with no game; the map loads (async). On failure: `deny {cmd:'start',reason:'scenario'}` to seats and the previous state returns.
3. `pause {paused:false}` broadcast; matchId+1.
4. `lobby` (state `play`, result null) to everyone.
5. If loaded from a save: `pause {paused:true, reason:'host'}`.
6. `start` to each human seat, each spectator (seat 0 view), each operative; each followed by `pause` when paused.
7. Only now may the tick loop send snapshots.

### 6.4 Play

- Snapshots `s` every `snapEvery` ticks (2 to 4, so 10 to 5 per second).
- Client to server: gameplay commands, `ping` every 2 s, map pings, `resync` on mismatch, operatives' `fps` every 50 ms.
- Server to client: `deny` for refused commands (at most 4/s), `pong`, relayed `ping`.

### 6.5 Pause

- Host `pause`: `pause {paused:true, by, reason:'host', left:0}` to everyone; sim, AI, commands and operative input stop; one `s` frame and one `pause` per second.
- Human drop in play (first time this match for that player): drop pause, `reason:'drop'`, `left` counts down from 30.
- Ends on host `resume`, the dropped player's return, `handAi`, the deadline, `restart`, `end` or a new start: `pause {paused:false}`.

### 6.6 Disconnect and reconnect

- Seat drops in the lobby: seat kept 10 s, then freed and `lobby` broadcast.
- Seat drops in play: seat kept offline (`away` each tick: no commands, no income), possibly a drop pause; freed when the match ends.
- Spectator drops: removed at once.
- Operative drops: kept, marked offline; soldier idles; no pause.
- Client retries after 1, 2, 4, 8, 8 s (10 s after `full`) and sends `hello` with the same token.
- Rejoin: same player object; the old socket (if still open) gets `replaced` and is closed; the seat's AI helpers reset; `lobby` to everyone; in play the returning client gets `start` (same `matchId`, so the browser restores camera and selection) and `pause` if paused, before its `lobby`; a drop pause for that player ends.
- A tab that receives `replaced` stops reconnecting until the user chooses "use seat here" (client/main.js:198, 204).

### 6.7 Leave and hand over

- `leave` (seat in play): AI takes the army, `left` to the sender, close; the client stops reconnecting.
- Host `handAi {slot}` for an offline seat: same handover without a message to the absent player.

### 6.8 End

1. A winner is set: everyone sees everything (`reveal`), commands are refused, AIs idle.
2. For 6 s the sim steps every other tick and every step sends a frame (`s` with `winner` and `end`).
3. `pause {paused:false}`; offline humans freed; `lobby` with `state:'lobby'` and `result` (each seat's own `result.you`).
4. Host `end` skips the hold: result `{ended: true}`.

### 6.9 Save, autosave, load

- Host `save` in play: `saved {id}` to the host and a `lobby` with the new save, or `deny save`.
- World autosaves every 2 minutes of play; the next `lobby` lists it.
- Host `loadSave` in the lobby: seats matched by token, the match starts paused (6.3); host `resume` to continue.

### 6.10 Room end

When no seat, spectator or operative is connected for more than 60 s, the room and its state are deleted. Saves on disk stay.

## 7. Process messages in shard mode (not WebSocket)

| Message | Direction | Fields | Evidence |
|---|---|---|---|
| `ready` | game process to front | `port` | server.js:813; server/shards.js:22 |
| `record` | game process to front | `key` (`map|teams|army`), `run` | server.js:84, 804 |
| `maps` | front to every game process | none (reread Horde maps) | server.js:127, 808 |

## 8. Version and compatibility

- **No protocol version or handshake exists.** `hello` carries no version and the server checks none (server.js:588-617).
- **Implicit tolerance in the browser client:** a snapshot without `held` (a server older than deltas) makes the client replace all rows (client/world-state.js:12-14); a `start` without `fog` means nothing is hidden (client/fog.js:26-27). Both exist so a stale server and a fresh client still work.
- **Static files** use ETag plus `no-cache`, so a browser picks up new client code on reload (server.js:172-178); nothing pairs client and server versions.
- **Saves** have no version field and use Node's V8 serialization (server/saves.js:18-27); a save that fails to read gives `deny {cmd:'loadSave',reason:'save'}`.
- **Data versions in payloads:** `scenario.version: 1` in snapshots (shared/scenarios.js:173); `worldVersion: 2` in World maps (shared/sim.js:1448); map files are migrated by `migrateWorldMap` (shared/world-layers.js:69, MAP owns it). Horde records have no version.

## 9. Unverified items

- The `ws` library's exact behavior for a wrong upgrade path, an oversized frame (close 1009), an offered subprotocol, a binary frame, and its default deflate parameters (ws 8.22.0).
- The full refusal reason chains on shared/sim.js:3640 (`dig`) and 3695 (`build`), which are long single-line expressions only partly read here; placement reasons come from `placementCheck` (ENG and BASE).
- The complete key list of `shots` and `flights` entries (CMB owns them).
- The close status a browser sees after `ws.close()` with no code (1005 expected, not checked).

### Recipient ground-list checkpoint

A source-written internal producer now retains complete observed wreck rows from canonical Environment metadata and Game hulls, preserves found-mine discovery order, and projects World nodes from actual team memory and ResourceNode owners. Ordinary nodes remain absent; missing imported wreck facts reject explicitly. Five original-source traces cover 90 actions. Native checks and complete-envelope delivery integration remain pending; this does not change the draft wire contract.
