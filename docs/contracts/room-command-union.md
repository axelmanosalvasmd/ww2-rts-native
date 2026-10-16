# Prototype command union through the room queue

Reference: `ww2-rts` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
Owner decision M in issue #5 comment 6217590741 permits the prototype commands.
World admission remains deferred. This note records existing JSON WebSocket v0
objects. It adds no message type, order kind or success reply.

The server resolves the current seated connection and supplies issuer, next tick
and sequence to the one core queue. Client authority fields have no effect.
Guests command their own units. The prototype applies no host restriction to
these ordinary commands (`server.js:712-715`). Pause drops new gameplay input
without consuming a queue key. Host controls, Horde break, sandbox spawn and
operative input retain their composed owners.

| Command object | Prototype source and behavior |
| --- | --- |
| `{"t":"attack","ids":[id],"target":target,"queue":true}` | `shared/sim.js:3499-3510`: visible enemy target; ground troops cannot attack aircraft; queued orders display kind 4. |
| `{"t":"ability","ids":[id],"x":x,"z":z,"queue":true}` | `shared/sim.js:3602-3628`: aimed coordinates and queue are optional; the unit ability owns cost, cooldown and eligibility. Queued aimed abilities display kind 6. |
| `{"t":"autocast","ids":[id],"on":true}` | `shared/sim.js:3629-3634`: `on` must be boolean; only units with an ability participate. |
| `{"t":"build","ids":[id],"kind":"flakpos","x":x,"z":z,"dir":angle,"queue":false}` | `shared/sim.js:3689-3709`: selected builders pay for a visible valid site; queued construction retains an Assist order. |
| `{"t":"board","ids":[id],"target":carrier}` | `shared/sim.js:3521-3544`: optional target; otherwise use selected carriers, preserving the prototype selection and seat rules. |
| `{"t":"unload","ids":[carrier]}` | `shared/sim.js:3564-3570`: selected carriers unload their cargo; boats require a reachable shore. |
| `{"t":"escort","ids":[plane],"target":friendly}` | `shared/sim.js:3515-3520`: aircraft circle a friendly ground unit. |
| `{"t":"rail","ids":[id],"x":x,"z":z}` | `shared/sim.js:3545-3563`: ordinary modes return `blocked`; World dispatch remains deferred. |
| `{"t":"orders","commands":[part],"queue":true}` | `shared/sim.js:3396-3405,3416`: at most eight allowed parts; the parent queue flag replaces each part's flag. Any success accepts the group; otherwise report the first denial, or `blocked` if no part participates. |
| `{"t":"cancelProduction","id":building,"job":job}` | `shared/sim.js:3737-3747`: construction modes cancel waiting jobs after index zero and refund their original MP and fuel. Active/nonwaiting jobs return `notWaiting`; non-construction modes return `needs`. |

Every raw troop selection reaches the canonical executor without server-side
filtering. The existing cap, foreign/missing/riding/operative exclusions and each
family's forced-logistics rules remain there (`shared/sim.js:3417-3421`). A mixed
selection does not become `unsupported` merely because a canonical aircraft or
construction participant is present. Canonical guards still refuse unsupported
state, including aircraft abilities, depot construction and deferred World state.

A denial uses the unchanged object
`{"t":"deny","cmd":"attack","reason":"unseen"}`, delivered only to the
issuing seat with the existing throttle (`server.js:713-714`). No success object
is sent.

Waiting abilities use the existing owner-only row
`[id,count,6,x,z,...]`, with coordinates rounded to tenths
(`shared/sim.js:3381-3385,5893-5894,6027`). Only normalized singleton,
nonautomatic, unqueued stored abilities with finite coordinates project through
this adapter. Active aimed plans already use kind 6.

An internally attached Classic match with logistics disabled now passes the same full room snapshot
path as Conquest. Public Classic start remains guarded; hosted Horde keeps its separate start owner. Classic settings,
authored map, initial terrain and elimination startup layout must agree before
attachment. Classic frames retain owner-only production identities and refunds
(`shared/sim.js:6023-6024`), owner MP/munitions/fuel and income
(`shared/sim.js:6017-6019`), the existing Classic mode object
(`shared/sim.js:5895-5898`), and public node rows `[x,z,rate,fuelFlag]`
(`shared/sim.js:5932,6019`). Canonical elimination and production validation
precedes delivery. No World snapshot or room admission is opened.


Classic natural finishes use the same result object keys as Conquest, with
`mode:"classic"`, `winVp:null`, the canonical `hq` reason and territory timeline
samples `{t,points,structures}`. Each seat receives its own `you` outcome. The
server retains play for 120 callbacks, advances the simulation and emits a
frame every second callback, then resumes and returns to lobby with the result
(`server.js:738-754`; `shared/story.js:66-70`). The lifecycle test ends through
an ordinary queued HQ destruction and canonical elimination, after exercising
production cancellation. It checks 60 ending frames for every seat, final
result keys and timeline, and stable post-result status. Public Classic start
remains guarded.
