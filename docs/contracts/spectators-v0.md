# Ordinary spectator delivery in JSON WebSocket v0

Baseline: prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
The owner clarification for group J on 2026-10-16 selects the reference behavior:
ordinary spectators see the whole map at the current tick, with no observer delay.
World Conquest alone uses seat 0's team view in the prototype. World admission
remains closed on R1, so this implementation does not admit that branch.
This documents prototype-identical fields and recipient rules. It adds no ABI or
new message type. Hermes' runtime acknowledgment remains specific to the PR head.

A `hello` takes an available lobby seat unless `spectate:true`; otherwise it takes
one of eight spectator places. During play every new token takes a spectator
place. Overflow sends `{"t":"full","reason":"started"}` in play, or reason
`"seats"` in the lobby, then closes (`server.js:609-611`). Reconnecting the same
token replaces its old socket. The controlled R1 host still requires two opposing
human commander tokens; explicit lobby spectators and running-match spectators
are admitted without changing that commander requirement.

Spectators receive seat 0's `start`: `t`, `matchId`, `map`, `you:0`, `spawn`,
`spawns`, `cells`, `names`, `teams`, `factions`, `weather`, `sandbox`.
`fog` is absent, not null. Terrain uses fresh spectator memory, independent of
seat 0 and the shared stream (`server.js:455-467`). A pause message follows the
start when paused; the asynchronous lobby response follows the join start
(`server.js:612-615`). Existing lobby additions `proto:0` remain unchanged.

The ordinary stream has one shared terrain memory and delta ledger. A send
builds and serializes one snapshot, then shares that encoded string with every
eligible spectator. A start after that stream exists forces its next unit and
list delivery full. `resync` also requests a shared full delivery. Delta keys
`units`, optional `gone`, optional `all:true`, `held:[count,xor]`, and optional
`wrecks`/`nodes` retain the existing v0 shapes (`server.js:217-245,266-284,457-458`,
`shared/sim.js:5953-5977`). Terrain deltas remain shared, while a join start
contains the fresh full terrain. No fog field or delayed frame is sent.

The projection removes exactly these keys (`server.js:247-264`):
`movement`, `queues`, `productionJobs`, `plans`, `orders`, `air`, `mp`, `inc`,
`mun`, `fuel`, `fuelInc`, `upkeep`, `sup`, `rally`, `home`, `works`, `covers`,
`logistics`. The prototype retains seat 0's `tech`; this implementation does too.
All live units are visible. Seat 0's cooldown is zero and its stance/autocast bits
`2048|4096|8192|16384` are cleared. Other owners use the cached snapshot mask
`2047|32768|(7<<20)|262144`, which clears gate-open, companion and cargo bits
(`shared/sim.js:5899-5904`). Unit target IDs and shot `f`/`t` identities remain
present after the named units die, including local-shot `fo`/`to` owner fields.
The spectator projection supplies `EVERYTHING.has` (`server.js:204,219-220`),
so `seenBy` retains even absent actors (`shared/sim.js:5945`); cached unit targets
and local/nonlocal shots follow that predicate (`shared/sim.js:5901,6002-6003,6083-6087`).
Seated recipients retain their existing visibility and identity-redaction rules.
Only public scenario objectives and authored shots reach spectators. Existing
ordinary capability guards still reject unsupported simulation state.

Terrain keeps the prototype's seat 0 team mine knowledge despite lifting fog:
unfound enemy mines remain hidden, and `foundMines` contains seat 0's team
findings (`shared/sim.js:5859-5884,6073-6077`). Pending terrain is rechecked only
when terrain or vision revisions change (`shared/sim.js:5846-5856`). Terrain, fog, pending events and
remembered lists owned by commander seats are not drained by a watcher read.

Spectator army commands produce no reply and never enter the command queue
(`server.js:712-714`). Latency `ping`, name changes and shared `resync` remain
available. A spectator whose queued output exceeds 4 MiB closes with code 1013
and reason `too far behind`; exactly 4 MiB is still eligible
(`server.js:266-273`). A reconnect rebuilds from a fresh start and forces the
existing shared stream full. Spectators remain in the room through the ending
hold and the result lobby.

Native checks: `spectator_tests` covers joins, exact start and close shapes,
command silence, shared current-tick delivery through ordinary victory and all
120 ending callbacks, asserting every seat's and both spectators' snapshot on
every tick. `recipient_snapshot::spectator` covers exact unit masks,
private-key omission, public shots, independent terrain and refusal atomicity.
NET-35 continues to use `watch_and_sit_obey_capacity_and_assign_fresh_seat_defaults`.
These checks do not prove World, operatives, save loading or complete logistics.
