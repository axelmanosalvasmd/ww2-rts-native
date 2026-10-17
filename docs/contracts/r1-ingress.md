# Ordinary R1 command ingress, JSON WebSocket v0

Compatibility source: `reference/ww2-rts` at
`e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. This extends command admission in
the existing private, two-human Conquest R1 room. It keeps the existing wire
shapes, canonical meters and recipient view. World, Tutorial, Horde, spectators,
operatives and World recovery remain outside this batch. Ordinary convoy
commands and FireAt are admitted by the ROOM-COMMANDS successor.

The owner decisions are `julio-owner-decisions-recorded-20261016`
(issue #5, comment 6217590741), groups F and M. Julio's later owner instruction
`julio-owner-merge-now-20261016` (6217727398) confirms the grant and says Axel's
later written line does not gate implementation. Axel also confirmed these
decisions on #5 in `axel-owner-confirm-a-to-p-20261016`. This follows the handoff
and preserves `axel-interface-response-004`. No Hermes ack is claimed here. The stream stops after PR creation; the
orchestrator owns later CI, review and merge decisions.

## Host controls and resync

All objects below are identical to the prototype shapes.

| Direction | Object | Prototype evidence and behavior |
| --- | --- | --- |
| Client to server | `{"t":"pause"}` | `server.js:700-701,481-485`. Current host seat only, during play. Repeated pause has no effect. |
| Client to server | `{"t":"resume"}` | `server.js:702-703,488-490`. Current host seat only, during play. Clears host or drop pause. |
| Server to clients | `{"t":"pause","paused":true,"by":"a","reason":"host","left":0}` | `server.js:470-479`. `by` is the host's name, `left` is zero for a host pause. |
| Server to clients | `{"t":"pause","paused":false,"by":"","reason":"host","left":0}` | `server.js:470-479,488-490`. Resume, including finish. |
| Client to server | `{"t":"resync"}` | `server.js:684-687`. Requests a full next snapshot for the sender's seat. Existing `all:true`, `units`, `gone` and `held` shapes are unchanged. |
| Client to server | `{"t":"end"}` | `server.js:696-697,561-565`. Host only during play, also while paused. Resumes and returns to lobby immediately. |
| Server to clients | Lobby field `"result":{"ended":true,"you":null}` | `server.js:348,561-565`. Host end has no story and no per-seat outcome, including after an earlier completed match. |

During host pause, simulation and ordinary commands stop. Every second, the
server sends the pause message and a current frame without advancing the match
(`server.js:509-519,712`). Guests cannot pause, resume or end. During unpaused
play those commands return the existing blocked denial; while paused their
ordinary input is ignored (`server.js:700-714`; `shared/sim.js:3812`).

## Existing core command families

R1 now routes these prototype objects to the existing core parser and recorded
queue. The server supplies the stable seated issuer, next tick and sequence;
client `slot`, `issuer`, `tick` and `sequence` fields cannot select authority.
Commands have no success reply, as in `server.js:712-714`. A denial is exactly
`{"t":"deny","cmd":"destroy","reason":"unseen"}` with the incoming command
name and the core's reason. The existing limit of four denials per second stays
in place (`shared/command-feedback.js:2-8`).

| Command | Existing JSON fields | Prototype dispatch |
| --- | --- | --- |
| `move`, `amove` | `orders:[[id,x,z]]`, optional `ids`, `queue`, `together`, `face` | `shared/sim.js:3421-3461` |
| `fireat` | `ids`, `x`, `z`, optional `queue` | `shared/sim.js:3462-3477` |
| `supply` | `ids` or `orders:[[id,x,z]]`, `target` | `shared/supply-convoys.js:573-610` |
| `logisticsResume` | `ids` or `orders:[[id,x,z]]` | `shared/supply-convoys.js:573-590` |
| `garrison` | `ids`, `x`, `z`, optional `queue` | `shared/sim.js:3478` |
| `stop` | `ids` | `shared/sim.js:3511-3514` |
| `retreat`, `stance`, `cover` | `ids`; `key`,`on` for stance; optional `queue` for cover | `shared/sim.js:3571,3574,3580` |
| `dig`, `entrench` | `ids`, `x`,`z`, `queue`; `kind`,`dir` for dig; `pattern`,`fort`,`x2`,`z2`,`join` for entrench | `shared/sim.js:3635,3651` |
| `assist` | `ids`, `id`, optional `x`,`z`,`queue` | `shared/sim.js:3710` |
| `tech` | `kind` (`tier`, `inf`, `guns`, `armor`) | `shared/sim.js:3723` |
| `cancel`, `destroy`, `rally` | Existing `id`,`ids`,`x`,`z` fields for the respective commands | `shared/sim.js:3748,3760,3766` |

`cancelProduction` remains refused before enqueue in R1 Conquest with
`{"t":"deny","cmd":"cancelProduction","reason":"needs"}`. This is exactly the
prototype's non-construction-mode result (`shared/sim.js:3737-3738`). Conquest
training queues remain forbidden by the native tick guard.

The socket denial matrix verifies a deterministic prototype denial branch per
command. Existing ground selection restrictions remain in place, and
client authority fields are ignored. Unknown orders in an active, unpaused seat
now receive the prototype's `blocked` denial (`shared/sim.js:3812`).

Garrison and cover retain the prototype semantics (`shared/sim.js:3420,
3451-3452,3478-3497,3580-3606`): the raw ID list is capped at
`max(50, unit count)` before filtering, queued cover after garrison keeps the
queued house coordinates, queued cover leaves ongoing entrenchment intact,
immediate cover cancels that worker's entrenchment, and Move or attack-move exits
a house through the canonical placement path. The cap audit also covers dig
(first ID only), entrench, cover, assist, retreat, rally and destroy. Research and
both cancellation commands use scalar inputs rather than a selected ID list.

The public lifecycle test sends commands over actual sockets from start through
successful garrison, dig, entrench, cover, assist, retreat, rally, tech, cancel
and destroy, public capture orders, VP victory, final hold, result and a subsequent
host end. Its initial construction fixture is declared separately from socket
input because public build remains closed. Every simulation tick builds snapshots
for both seats and checks for a fault.
Normal Conquest results retain `winner`, `teams`, `names`, `reason`, `at`,
`endTick`, `story`, `timeline`, `winVp`, `mode` and recipient `you`, as in
`server.js:348,753` and `shared/story.js:66-70`. This proves the Conquest and
host-end portions of MODE-90; other modes and Horde results remain unqualified.

## Room command composition

FireAt preserves the prototype JSON object and denial reasons (`blocked`,
`notVisible`, `needs`, `queueFull`). Salvo weapons can target unseen open ground;
direct terrain guns require a cell with hit points. Ground queues retain their
existing owner-only order kind 5 (`shared/sim.js:3381-3390,5894,6027` and native
`recipient_snapshot/owners.rs`). Bombers take an immediate area mission even
when `queue:true`; a mixed bomber and ground selection applies both behaviors
(`shared/sim.js:3462-3477`). Finite coordinates use the prototype's clamp to map
extent (`shared/sim.js:3258`); an exact upper extent lies outside the cell grid.

Convoy selection runs before ordinary troop selection. It preserves raw order
and duplicate IDs without the troop cap, uses `ids` when it is an array, and
otherwise derives IDs from `orders`. Only live trucks owned by the seated issuer
participate. Supply targets must be live friendly reserve-bearing units or
completed supply caches. Missing selection/targets return `needs`; a valid
selection whose route/job cannot be assigned returns `blocked`. Without enabled
convoys these commands return the prototype fallback `blocked`. Supply and resume
use the existing recipient-fed routes, stores, reserves and convoy state. No new
snapshot fields are introduced. `logisticsResume` clears manual/hold state and
restarts the retained job (`shared/supply-convoys.js:573-610`).

Every admitted command uses the canonical next-tick queue. Guests may issue
FireAt and convoy commands for their own units. Host-only restrictions remain
on the prototype host controls; ordinary commands have no extra host gate
(`server.js:696-715`). Paused gameplay is ignored; spoofed authority fields never
select a slot, tick or sequence. Denial delivery and throttle are unchanged.
Pending convoy inputs survive the existing versioned JSON checkpoint.

## Held dependencies

Sandbox, attack, ability, autocast, build, board, unload, escort, rail and grouped
orders retain their public guards on this composition. World recovery stays
deferred under decision C. NET-43 therefore remains partial, despite the three
additional command families. The attempted #393 merge was aborted because its
sandbox-only flight adapter competes with #369's general ordinary adapter; see
the complete merge record in `docs/testing/room-commands-20261016.md`.

The #296 base previously limited support to Artillery, Smoke and Cover and
refused aircraft purchases because snapshots could not carry flights. Integrated
#369 now supplies all eight support kinds and the flight/event projection.
This does not open the other guarded public command families. NET-58 is qualified
only for the ordinary #369 event and flight families; operative small-arms and
Horde breath events still retain their separate admission guards.
