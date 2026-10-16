# Construction and production recipient fields

Compatibility source: `reference/ww2-rts` at
`e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. These JSON WebSocket v0 fields
match the prototype. This documents the internal Rust producer and trusted
Classic room attachment; public Classic lobby start and Unreal adoption are
separate work. The approved ABI 2 header is unchanged.

The implementation follows the prototype wire and privacy decisions recorded
in issue #5 comment 6217590741, confirmed by Axel in comment 6218552386.
Hermes' runtime acknowledgment still applies to the final PR head.

| Field | Exact shape and recipient rule | Prototype source |
| --- | --- | --- |
| `queues` | `[buildingId, progress, rallyX, rallyZ, ...unitTypes]`. Progress is head elapsed time divided by its train time, rounded to one decimal, or zero for an empty queue. An absent building rally is `-1, -1`. Ordinary and Classic cached rows belong to the building owner. Horde additionally exposes allied defender producers, excluding the Horde enemy. | `shared/sim.js:387,5916-5917,6022` |
| `productionJobs` | `[buildingId, [{id, type, mp, fuel, status}, ...]]`. The head is `active`; later entries are `waiting`. Charges remain the original paid MP and Fuel. Only the building owner receives these rows, including when an ally can use its producer. | `shared/sim.js:1034-1050,5916,6024` |
| `tech` | `{tier, armory: {inf, guns, armor}, lab: [[kind, progress], ...]}` for the recipient's research. Lab progress is rounded to one decimal. Omitted when technology is disabled. | `shared/sim.js:6018` |
| `nodes` | Classic publishes every node to every seat in canonical order, including unexplored cells: `[x, z, rate, fuelFlag]`. Coordinates are rounded to one decimal; rate is unrounded; fuelFlag is `0` or `1`. Depot identity and ownership are absent. World keeps its separate explored-team filter. | `shared/sim.js:5932,6019` |
| `rally` | The recipient's personal `[x, z]`, rounded to one decimal, or `null`. It is independent of building rallies. | `shared/sim.js:6028` |
| `mun`, `fuel` | Classic initializes both resources, so the recipient receives their floored wallet values. Ordinary Conquest's absent resource fields stay absent. | `shared/sim.js:1682,6017` |
| `mode` | Classic sends `{kind: "classic", timeLeft, suddenDeath}`. Time is clamped to zero after rounding up to whole seconds. No `teams` or node claim data is included. | `shared/sim.js:1679,5895-5897` |
| `ghosts` | Classic sends an array, including `[]` before any enemy building is remembered. Remembered buildings retain the existing six-field rows and team visibility filter. | `shared/sim.js:5928-5930,6037` |

Waiting-job cancellation remains owner-only and is refused during Sudden Death.
It refunds the stored charge and preserves head progress. A rejected active-job
cancellation returns `notWaiting` (`shared/sim.js:3737-3747`). These commands
continue to use the existing trusted recorded-input boundary; this change adds
no public command union or lobby-start mode.

The Classic producer, node publication and recipient privacy paths are covered
by native tests in `recipient_snapshot::production_state_tests` and the trusted
room lifecycle test. BASE-59 remains incomplete until the logistics and hosted
Horde runtime streams provide their mode-specific delivery paths.
