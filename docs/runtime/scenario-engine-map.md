# Prototype scenario engine map

At reference/ww2-rts e4e579a, `shared/sim.js:11` imports the evaluator from `shared/scenarios.js`.

| Kind | Fields | Effect | Prototype line | Rust before this change |
| --- | --- | --- | --- | --- |
| time | seconds | Match time reaches threshold | scenarios.js:113 | Admitted |
| ownership | target, side | Current point slot or team ownership; region team requires World | scenarios.js:114 | Typed component only |
| capture | target, side | Historical capture by slot/team | scenarios.js:115 | Typed component only |
| unitLost | unit | Authored ID was published and is now dead/absent | scenarios.js:116 | Typed component only |
| groupLost | group | Started, no living members or pending arrivals | scenarios.js:117 | Typed component only |
| groupCount | group, op, count | Living member count comparison | scenarios.js:118 | Typed component only |
| areaEntry | area, group or side | Living member/side unit in inclusive cell box | scenarios.js:119 | Typed component only |
| objectiveComplete | objective | Context completion, then match fallback | scenarios.js:120 | Typed component only |
| triggerComplete | trigger | Context execution count, then match fallback | scenarios.js:121 | Typed component only |
| all / any | conditions | Short-circuit conditions under shared budget | scenarios.js:111-112 | Typed component only |
| say | text, recipients, side | Localized recipient-filtered shot | scenarios.js:159-160 | Admitted match/once only |
| objectiveActivate / objectiveComplete | objective, recipients, side | Durable context HUD; complete is final | scenarios.js:161-164 | Admitted match/once only |
| reinforce | group, side, roster, at, order, expires | Deferred roster, absolute expiry, hold/move/attackMove | scenarios.js:165 | Not connected |
| damage / destroy | box, damage | Canonical terrain/section damage per cell, destroy=100000 | scenarios.js:166; sim.js:4913-4918 | Legacy Destroy only |

Initial authored units queue at setup (scenarios.js:93-98). Deferred passes run before/after triggers, 256 attempts per pass, expiry or eliminated side cancels (124-135,141,169). sim.js:4893-4912 checks current passability, population/type limits, first free radius 0..3 cell in dy/dx order, and canonical spawn/orders. Reinforcement preflight uses initial terrain (sim.js:4885-4891); authored units use startup current terrain (817-818). Faction/mode/naval availability is sim.js:4920-4923. Startup errors preserve the previous server room at server.js:364-389.

Contexts, repeats and action batch rules: scenarios.js:100-104,143-157. Tick hook after support and before weather: sim.js:5638. Public/side/team recipient rules and shapes: scenarios.js:87,159-175 and sim.js:6034,6080.
