# Tutorial and public Start on JSON WebSocket v0

Owner decision M in issue #5 comment 6217590741 authorizes prototype commands on R1. This implementation uses reference `e4e579a838aabae3b1d9dadbc17a0a87147bc52d` without changing the FFI or JSON protocol version. World room admission remains closed.

A new room's `hello` with `listing.tutorial === true` is unlisted, uses mode and map `tutorial`, and starts when the first commander joins. The creation-only condition is from `server.js:592-616`; reconnecting with that flag does not restart a match. Selecting Tutorial in a lobby fixes the map, and later map commands are ignored (`server.js:653,659-661`).

Public `start` requires the current commander host, a lobby, and one through the map's seat capacity. Assault also requires at least one defender and one attacker (`server.js:331-336,682-683`). Invalid preconditions are ignored. Preparation failure returns the existing `{"t":"deny","cmd":"start","reason":"scenario"}` (`server.js:379-385`). Conquest, Tutorial and Assault use their canonical constructors. Other mode executors, AI seats and spectators retain their existing runtime guards. The trusted private-room policy keeps its existing opt-in restrictions.

## Startup

The existing start envelope is unchanged (`server.js:456-466`): `t`, `matchId`, `map`, `you`, `spawn`, `spawns`, `cells`, `fog`, `names`, `teams`, `factions`, `weather`, `sandbox`. The human seat remains `you`. Tutorial includes the unattended Enemy in the full game roster arrays. Human names override the corresponding core names; Enemy retains its core name. Tutorial human teams are zero, Enemy's team is one, and all humans share the first authored home (`shared/sim.js:688-708`). Enemy stays outside the lobby roster and receives no socket or AI scheduler.

Startup emits the existing unpaused message, playing lobby, then each seat's start (`server.js:391-410`). Tutorial hello first emits its initial lobby. Runtime `online` and `ping` arrays describe only room seats, excluding the unattended Enemy (`server/room-tick.js:30-35`).

## Snapshot mode and dialogue

These shapes are identical to `shared/sim.js:5895-5897,6032`:

```json
{"kind":"tutorial","suddenDeath":false,"slot":1,"goal":"Move your squads up to the hedgerow","step":0,"steps":9}
```

`slot` is the final game player index, `step` is zero-based, and `goal` follows the canonical Tutorial step. There are no defender, attacker, time or bunker keys in the Tutorial mode object.

Assault uses `kind`, `defenderTeam`, `attackerTeam`, `timeLeft`, `suddenDeath:false` and `total`. `timeLeft` is the nonnegative ceiling of the remaining seconds. `total` is the startup bunker count (`shared/sim.js:787-790,832-840,5895-5897`). No absent optional mode keys are filled with null.

Tutorial dialogue is a public entry in the existing `shots` array:

```json
{"k":"say","text":"Drag a box over the boys to select them, then right-click to move. Hedgerow first.","x":27,"z":45,"pub":true}
```

The producer uses the step's map-cell centre, begins delivery at tick 40, and retains emitted lines until the scheduled frame (`shared/tutorial.js:84-86,109-115`). These public non-World rows reach every selected recipient regardless of fog (`shared/sim.js:6080`). Current/catch-up frames do not replay the scheduled event batch. The core tick batch retains dialogue with other chronological events; the existing save codec can retain that batch without a separate dialogue store.

## Ending

Tutorial finishes through the existing match finish and 120-callback hold. Result arrays retain every game player, including Enemy, while human outcomes remain attached to lobby seats (`server.js:741-754`). Tutorial and Assault use the prototype's non-Conquest timeline rows `{"t":...,"points":[...],"structures":[...]}` (`shared/story.js:13,53-62`). The result envelope remains the existing `storyResult` shape. No field is added to the wire beyond these existing prototype mode and dialogue shapes.

`MODE-90` and `MODE-96` are completed by the composed results stream. The literal Tutorial journey checks every mode object against all nine prototype goals, omits `end` until victory, and delivers `{reason:"tutorial",x,z}` and the complete `out` array through all 120 ending callbacks. The result has exactly `winner`, `teams`, `names`, `reason`, `at`, `endTick`, `story`, `timeline`, `winVp:null` and `mode:"tutorial"` (`shared/story.js:66-70`, `server.js:753`). Human names come from the current room roster after reconnect; the unattended Enemy keeps its core name. Each seated human receives its own `result.you`; host `end` publishes only `{ended:true}` with `you:null` and no story (`server.js:348,696-697`), including after a prior completed match. Tutorial remains human-only at attachment.

The focused server tests cover auto-start/reconnect, one-to-five-human Tutorial starts, public Start capacity and Assault readiness, and Tutorial progression through dialogue, garrison, support, victory and result delivery. Snapshot projection is exercised for every game seat. This does not claim complete gameplay ingress or open the retained unsupported actor, AI, spectator or logistics paths.

## Active projectiles

Owner groups E/O cover the projectile state needed when Tutorial and Assault units fire. Active flights use the prototype's `flights` array (`shared/sim.js:5970-5995`):

```json
{"flight":1,"kind":"rifle","seq":0,"x":35,"y":10,"z":35,"v":[10,0,0],"gravity":0,"time":6.25,"launched":6.25,"expires":8.25,"until":6.4,"f":1,"fo":0}
```

Each row uses the canonical current position and velocity, launch/expiry times, sequence and unit type. `effect` is omitted when absent. A future launch is omitted. The ground-cell fog mask gates each row even for its owner; `reveal` overrides that mask. `f` and `fo` appear only when the shooter is visible. No target identity is sent. `until` limits extrapolation to 150 ms, expiry, the first hidden cell, and a visible static contact using current poses. Projection does not advance projectiles or damage actors.

Private salvo schedules and grenade tickets do not add wire arrays. Their physical flights and emitted events already represent them (`shared/sim.js:4141-4152,6079-6080`). Snapshot admission validates those owners and retains the guard for undrained producer event buffers. The component test checks the exact flight object, hidden-shooter redaction, unseen and future flight exclusion, and static contact clipping. The room lifecycle test retains active artillery while collecting every recipient snapshot.

Ordinary grenade throws also use the existing prototype shot shape from `shared/sim.js:5427` (shared with operative throws at `:2104`):

```json
{"f":1,"fo":0,"x":35,"z":35,"k":"throw","physical":true,"local":true}
```

The existing local-event rule applies: the ground cell must be visible, and `f`/`fo` are removed together when the shooter is hidden (`shared/sim.js:5998-6003`). This admits the ordinary Tutorial ability event without opening operative room admission.
