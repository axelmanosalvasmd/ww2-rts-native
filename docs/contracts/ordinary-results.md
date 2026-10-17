# Ordinary mode snapshots and results

Baseline: prototype `e4e579a`. Owner groups N and P in issue #5 comment
6217590741 allow the existing prototype wire shapes and recipient rules.
This document records identical JSON WebSocket v0 fields. Hermes' exact-head
runtime acknowledgement remains pending. No C interface or header changes.

`recipient_snapshot` and trusted native attachment accept Default/Conquest,
Assault, Annihilation and Classic. Public R1 start still selects Conquest.
World and Tutorial snapshots remain guarded; hosted Horde is described in [hosted-horde.md](hosted-horde.md).

## Snapshot fields

The public `mode` row is identical to `shared/sim.js:5895-5897`, consumed by
`snapshotFor` at `:6032`. JavaScript fields whose values are `undefined` are
omitted from JSON, rather than sent as null or filled with defaults.

| Mode | JSON fields |
| --- | --- |
| Default/Conquest | `mode` omitted |
| Assault | `kind: "assault"`, `defenderTeam`, `attackerTeam`, `timeLeft`, `suddenDeath: false`, `total` |
| Annihilation | `kind: "annihilation"`, `suddenDeath: false`, `bunkers` |
| Classic | `kind: "classic"`, `timeLeft`, `suddenDeath` |

`timeLeft` is the ceiling of the canonical remaining seconds, clamped to zero.
`total` is the initial Assault bunker count (`sim.js:788-789`). `bunkers` is the
initial Annihilation count indexed by team, including zero-filled team gaps
(`:791-796`). Neither count is replaced with a live count after destruction.
Classic's timer stops at victory; sudden death survives into the ending hold
(`:5756-5765`).

Every seat gets `out`, an array of booleans in seat order. Before victory, `end`
is omitted. Once decided, it is exactly `{reason, x, z}` from the canonical
finish, while `winner` contains the winning team or -1 for a draw
(`sim.js:5807-5814,6017-6019`). These are public facts for all seats. Unit,
order, fog and private-wallet recipient rules retain their existing owners.

Classic also publishes `mun` and `fuel` rounded down, because Classic creates
those wallets (`sim.js:1682`), and `nodes` as `[x, z, rate, fuelFlag]` rows in
canonical node order. Positions round to one decimal place; `fuelFlag` is 0 or
1. Nodes are public in ordinary modes (`:5932,6018-6019`). Classic also emits
`ghosts: []` before any building memory exists (`sim.js:5928,6037`). Other admitted
modes omit these fields when the prototype leaves them undefined. The raw node
list also participates in the existing server list-version cache.

## Result publication

After 120 ending callbacks, including 60 half-speed simulation steps and their
snapshots, the server releases the runtime and publishes the result in each
seat's lobby (`server.js:735-754`). Its exact common shape is:

```text
{winner, teams, names, reason, at: {x, z}, endTick,
 story: [{kills, losses, built, captures, mpSpent, supportCalls, planesDowned}],
 timeline, winVp, mode, you}
```

`story` has one row per seat, with every counter rounded using JavaScript's
rounding rule. The timeline is `{t, vp}` for Conquest and
`{t, points, structures}` for the other ordinary modes. Team IDs index the arrays
with gaps preserved; `winVp` is a number for Conquest and null for the other
ordinary modes (`shared/story.js:49-70`). Server seat names override the core
names (`server.js:753`). The existing roster owner moves result rows when seats
leave or join (`:533-557`).

`you` is the recipient's `{team, outcome}` where outcome is `victory`, `defeat`
or `draw`. It is withheld until result publication (`server.js:342,752-753`).
No story or timeline is exposed in a live recipient snapshot.

Host `end` releases a healthy attached ordinary match, clears pause and sends
`{ended: true, you: null}` in the lobby result, without story or other natural
result fields (`server.js:561-565,696-697`). Only the ready current host may
use it. This also cancels an asynchronous pending start, as described in the
NET-37 contract. It does not authorize recovery from a latched runtime fault.

## Validation

`results2_tests::mode96_ordinary_modes_deliver_exact_rows_through_victory_and_full_hold`
checks exact field sets through Assault structure and timer victories,
Annihilation elimination, and Classic elimination and sudden death.
`results2_tests::mode90_complete_ordinary_results_publish_story_timeline_and_each_seat_outcome`
checks every common report field and both recipients, including a Classic draw.
Each path starts through native preparation, runs ordinary ticks, collects both
complete recipient snapshots every callback, and finishes the 120-callback hold
without a fault. Host termination has a separate live-match test.
