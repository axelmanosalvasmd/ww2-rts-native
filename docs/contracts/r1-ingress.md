# Ordinary R1 command ingress, JSON WebSocket v0

Compatibility source: `reference/ww2-rts` at
`e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. This extends command admission in
the existing private, two-human Conquest R1 room. It keeps the existing wire
shapes, canonical meters and recipient view. World, Tutorial, Horde, spectators,
operatives and convoy command admission remain outside this batch.

The owner decisions are `julio-owner-decisions-recorded-20261016`
(issue #5, comment 6217590741), groups F and M. Julio's later owner instruction
`julio-owner-merge-now-20261016` (6217727398) confirms the grant and says Axel's
later written line does not gate implementation. Axel also confirmed these
decisions on #5 in `axel-owner-confirm-a-to-p-20261016`. This follows the handoff
and preserves `axel-interface-response-004`. Hermes' exact-head content review
remains required; no ack is claimed here.

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

## Held dependencies

Sandbox, attack, fireat, ability, autocast, build, board, unload,
escort, rail and grouped orders retain their existing public refusal. Tutorial
auto-start, convoy commands and recovery are also deferred. The prototype's
research message is named `tech`; no `research` message is added.

Broader ingress cannot safely open against this base: persistent firing flights,
salvos and grenades are refused by
`rust/crates/core/src/recipient_snapshot/composition.rs` in `admit_actors`, while
`match_firing.rs` can create those actors after an accepted attack or aimed
ability. Plane-backed support similarly exceeds the adapter's
`SUPPORTED_SUPPORT_KINDS`. The flights/events stream must carry those complete
lifecycles before these command families or unrestricted sandbox births open.
Queued ability and fireat rows also need their prototype owner-only order
encodings before their waiting state can be delivered.

## PR 296 integration

The RESOLVE-296 successor carries the ground-order and host-control behavior
from #366 at `21a1e3fa`, including its core cover and garrison repairs and
socket lifecycle test. Attack retains #366's `blocked` refusal on this base;
#296's older `unseen` expectation is superseded until #369 delivers flights.

Two #296 families remain additional to #366: `buy` (`unit`, optional `from`,
`shared/sim.js:3781-3802`) and `support` (`kind`, `x`, `z`,
`shared/sim.js:3680-3688`). They use the existing parser and core denial feedback.
Aircraft purchases remain refused by the core's ordinary-snapshot guard.
Only Artillery, Smoke and Cover support pass the snapshot lifecycle allowlist;
all other kinds refuse with `unsupported` before reserving input. This keeps
main's actor, lifecycle and World guards intact. NET-43 remains partial.
