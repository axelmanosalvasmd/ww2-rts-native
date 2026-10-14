# Parity inventory

This is the checklist issue #4 asks for: every behavior and piece of content the JavaScript game implements at the reference pin, each with its evidence, what the Rust core and server must do, what the Unreal client must do, what it depends on, the check that proves it, and a status. The native game is complete when every row is accepted, changed with approval, or dropped with approval.

## How parity is judged

The JavaScript game is a prototype: it was built to try the game's ideas out, and nothing in it is fixed. It guides the native game; it is not a specification to copy bit for bit. Each row is something the prototype does, to be carried over with the same intent, improved, or dropped. Where Rust allows a better architecture or a cleaner rule, the native game takes it.

- **Inside the engine, improve freely.** Data layout, threading, scheduling, randomness, math and caching are Julio's to redesign. Record each choice in `DESIGN.md`.
- **What players notice needs both owners.** A change to a rule, number or outcome is recorded in `DESIGN.md`, its row is marked `differs`, and Axel agrees on #5 before it ships (issue #4).
- **Compare exactly where it is cheap.** Data tables, pure functions (prices, placement, formations) and single-component fixtures (one movement step, one grid query, one vision pass) are compared with the pinned JavaScript value for value, because that finds port bugs quickly.
- **Compare whole matches by outcome.** Once the native core uses its own random streams and schedule, a whole match cannot repeat the JavaScript one tick for tick. Matches are compared by outcomes and statistics agreed before the run (winner, duration, losses, resources, path times), as issue #4 section 5 describes.

Some rows were written before this rule and propose copying a JavaScript accident exactly (for example IF-20: one shared random stream, discarded draws included). Where a row and the decisions below disagree, the decisions win.

**Status: draft from the Rust side, 2026-10-12.** Julio's agents wrote the rows by reading the code at the pin; `DESIGN.md` and `CHANGELOG.md` served only to find things, and the code decided. The "Unreal (Axel)" column is a first guess written from the Rust side; Axel owns it and may rewrite it. Axel's 140-row client presentation/input inventory is published as [`docs/parity/client.md` at `9551d3976e9145c81030cf5a81ec7f0764b04d12`](https://github.com/axelmanosalvasmd/ww2-rts-native/blob/9551d3976e9145c81030cf5a81ec7f0764b04d12/docs/parity/client.md) in draft [PR #201](https://github.com/axelmanosalvasmd/ww2-rts-native/pull/201), explicitly not for merge. That file is not on main, and no client row is accepted. Its Axel-reported Linux build/automation on Rust `92fb7fe` is separate from live/package evidence on `03a49ac`. Fresh live adoption, Windows/macOS client and input checks, physical mouse/keyboard tests, complete decoder adoption and broader rules-query integration remain pending.

## Reference pin

- **Pin:** ww2-rts `e4e579a`, the `reference/ww2-rts` submodule. Every evidence path in `docs/parity/` is relative to that folder at that commit.
- **Decision so far:** `e4e579a` stays the working pin for the step 1 tests (#2, #3) and for this inventory. Julio and Axel choose the porting pin together at the #1 decision (Julio's ack `julio-coordination-001` on #5).
- **Later ww2-rts commits, as of 2026-10-12:** one, `d71532c` (#66). It speeds up the browser client's terrain, water, structure and minimap drawing; adds an opt-in WebTransport motion lane (`prototype/webtransport`, `server/webtransport-config.js`); makes invite links advertise the Tailscale IPv4 address; and fixes save loading, so a loaded match no longer steps or sends a snapshot before every player has its start. In `shared/`, `SpatialGrid.candidates` converts its result in place, and `houseNames` and `houseBlock` replace `houseKindsWhere`. Both give the same results by inspection; no parity run has confirmed it. The client now imports `houseBlock` and `houseNames` instead of `houseKindsWhere`.

## Sections

| File | Prefix | Scope | Rows |
| --- | --- | --- | --- |
| [modes.md](docs/parity/modes.md) | MODE | Lobby settings, match start, Conquest, Assault, Annihilation, Classic, Horde, Tutorial, World Conquest, scenarios, results | 103 |
| [maps.md](docs/parity/maps.md) | MAP | Map format, the 33 authored maps, validation, World generation, regions, terrain data and queries | 140 |
| [combat.md](docs/parity/combat.md) | CMB | Factions, the 47 unit types, weapons, abilities, autocast, stances, orders, formations, vision and fog | 160 |
| [movement.md](docs/parity/movement.md) | MOV | Movement, pathfinding, traffic, spatial grid, roads, rails, transport, aviation, naval, off-map support | 108 |
| [bases.md](docs/parity/bases.md) | BASE | Currencies, income, prices, population, buildings, construction, production queues, tiers, Armory, placement | 59 |
| [engineering.md](docs/parity/engineering.md) | ENG | Fortifications, garrisons, bridges, mud, mines, destruction, fire, weather | 80 |
| [logistics.md](docs/parity/logistics.md) | LOG | Reserves, shortages, territory supply, convoys, caches, supply lines, upkeep | 55 |
| [ai.md](docs/parity/ai.md) | AI | What the AI sees, how it orders, scheduling, difficulty, behavior per mode, randomness | 66 |
| [server.md](docs/parity/server.md) | NET | Rooms, seats, roles, spectators, snapshots and privacy, reconnect, pause, saves, shards, HTTP endpoints | 97 |
| [fps.md](docs/parity/fps.md) | FPS | The first-person companion: entry, permissions, input, weapons, squad, death and respawn | 40 |
| [interface.md](docs/parity/interface.md) | IF | What the client needs from the rules through the C interface, and the determinism rules the core must meet | 31 |
| [tools.md](docs/parity/tools.md) | TOOL | Developer tools and the test suite: port, reuse, replace or retire; parity harness building blocks | 65 |
| [protocol-messages.md](docs/parity/protocol-messages.md) | | The WebSocket protocol as implemented: 60 client-to-server and 11 server-to-client message types | |
| [client-imports.md](docs/parity/client-imports.md) | | Every client import from `shared/`, and every rule the client copies instead of importing | |

Each section ends with the existing limitations it found, the roadmap items it excluded, doc and code contradictions, and open questions for the owners.

## How to use it

- **Status values:** `todo`; `in progress`; `ported` (the Rust side passes the row's check); `accepted` (both owners signed off); `differs` (an approved difference, recorded in `DESIGN.md`); `dropped` (an approved removal). The tools table records a proposed decision per tool instead.
- **Row IDs are stable.** A new row takes the next free number; a removed row is marked `dropped`, not renumbered, so tasks and PRs can cite IDs.
- **Moving the pin** updates the rows its diff touches, in the same PR as the port and the parity run (`AGENTS.md`).
- **Limitations are decisions, not tasks.** Each section lists the known bugs and leaks at the pin. The native game may fix them; a fix players would notice follows the rule above.

## What the inventory found

1. **Live JavaScript matches cannot be replayed.** Combat rolls, artillery and strike scatter, flak, spawn layout and some AI choices draw from the global `Math.random`, which the server never seeds and every room in a process shares. The match's own seed (`g.seed`) and the weather seed come from it too. Orders apply when they arrive, between ticks. How often an AI seat gets a fresh view follows the server's measured tick time (`snapEvery` moves between 2 and 4). Only seeded headless runs repeat: `playMatch({ seed })` and `tools/bench.mjs`. See IF-20, IF-21, IF-28, IF-29 and AI-15.
2. **Neither Rust's standard math nor the `libm` crate matches V8.** Measured on 1,000,000 inputs per function (Node 24.21.0 with V8 13.6.233, rustc 1.98.1, `libm` 0.2.16, this Mac); each cell counts inputs whose result differs from V8's in the last bits:

   | Function | Rust `f64` (platform library) | `libm` crate (pure Rust) |
   | --- | --- | --- |
   | `hypot` | 379,300 | 374,377 |
   | `tan` | 383,747 | 11,158 |
   | `atan2` | 177,577 | 1,589 |
   | `exp` | 97,471 | 457 |
   | `cos` | 46,964 | 13,579 |
   | `sin` | 43,564 | 14,017 |
   | `log` | 19,364 | 1,724 |
   | `pow` (`**`) | 0 | 96,431 |
   | `sqrt` | 0 | not tested |

   A Rust copy of V8's `hypot` formula matched on all 1,000,000. V8 evaluates `**` with the operating system's `pow`, so the JavaScript game itself can differ between macOS, Linux and Windows. `Math.round`, `max`, `min` and `sign` also differ from their Rust namesakes at halves, NaN and negative zero. See IF-15 to IF-19.
3. **Order and 32-bit floats decide outcomes.** Some unit choices take the first unit in the spatial grid's bucket order; A* keeps path costs in a `Float32Array`; cell hit points and wear are 32-bit floats; every collection iterates in insertion order. The coarse route graph depends on its update history, so a reloaded save can route differently from the same match played straight through. See the determinism notes in [movement.md](docs/parity/movement.md) and [interface.md](docs/parity/interface.md).
4. **The reference suite passes, and the benchmark repeats.** `node test.js` at the pin: 348 checks passed, 0 failed, 118 s on an Apple M3 Max with Node 24.21.0 (the machine was busy, so this is not a timing). `tools/bench.mjs` (Conquest on Six Fronts, seed `0x3c1055`) gave the same final-state hash twice over 6,000 ticks, with 39,377 random draws and 355 units. It cannot reach 2,000 units: the population cap stops purchases, so the #2 harness must place copies directly (TOOL, "Parity harness building blocks").
5. **Privacy gaps exist at the pin.** Outside World Conquest, anyone who joins a running match with a new token becomes a spectator and sees the whole map. The companion's status sends a hidden attacker's position to the metre. Terrain changes inside the fog reach every client, and a cut supply line tells its owner an enemy is on the road. See the limitations in [server.md](docs/parity/server.md), [fps.md](docs/parity/fps.md) and [combat.md](docs/parity/combat.md).
6. **Saves are tied to Node.** A `.save` file is `v8.serialize` output with no version field. The commander AI's state and the path caches are not saved, so a loaded match does not continue exactly.
7. **The docs lag the code in 73 places.** Examples: World Conquest is won with 60% of the regions, not all of them; the Classic population cap is 24, not 20; the companion's rifle does 8 damage every 0.32 s to 60 m, not 6 every 0.65 s to 36 m; generator v3 is labeled unreleased but runs in every World match. Each section lists its own.
8. **Live rooms and the tools run different rules.** Live rooms turn logistics and HQ tiers on; the headless tools and most seeded tests leave both off.

## Proposed decisions

These follow the rule above: the prototype shows what the game does, and the native game improves on it wherever it can.

For Julio (inside the Rust core and server):

1. **Comparison runs.** Seeded headless JavaScript runs (`playMatch` with a seed, the `bench.mjs` setup) produce the component fixtures and the outcome baselines. Live JavaScript sessions are not replayed.
2. **Randomness.** One seed per match, recorded with the match. Separate seeded streams per purpose (combat, off-map support, environment, each AI seat), never one stream shared across rooms, and no discarded draws. A match then replays from its seed and its order log, which also gives the replays `DESIGN.md` wants for cheat reports. Whole matches stop matching the JavaScript tick for tick, so they are compared by outcome.
3. **Math.** One math module in the core that gives the same bits on Windows, macOS and Linux, because the server, the Unreal client's previews and replays must agree. Copying V8's algorithms (its `hypot` formula, its fdlibm functions) costs a few hundred lines and keeps component fixtures exact against the JavaScript; the `libm` crate is simpler but differs from V8 on up to 37% of inputs (table above).
4. **A fixed schedule.** The AI's view refreshes on a fixed tick beat, whatever the server load; orders received between ticks apply at the next tick boundary in a defined order. How often snapshots go out may still adapt to load, separately from the AI.
5. **Explicit order.** Ties break on a defined key (for example unit id), not on the spatial grid's bucket order or the history of the route cache. A saved and reloaded match then continues exactly.
6. **World generation in Rust**, using the core's math, so the server needs no Node. Generated maps need not equal the JavaScript ones bit for bit; they must pass the generator's checks (connected land, river and region counts, home spacing, the fairness tests).
7. **Saves.** A new versioned format that stores everything needed to continue a match exactly, AI commander state included. Old `.save` files are not imported unless both owners ask for it (issue #4 asks for an explicit decision).
8. **Test configurations.** Checks run with logistics and HQ tiers on, as live rooms play, and off, as the tools play.

For Julio and Axel together (players would notice):

9. **Known bugs and leaks.** Proposal: fix them in the native game and record each fix, rather than port them. Candidates: spectators who see the whole map, the companion's view of a hidden attacker's position, terrain changes sent from inside the fog, Tutorial income and its missing defeat, Sudden Death queues emptied without a refund, rail track laid on fords, and the others listed per section.
10. **Coordinates.** [maps.md](docs/parity/maps.md) proposes Unreal X = x × 100, Y = z × 100, Z = height × 100 (centimetres), which keeps the simulation's angles as Unreal yaw with no sign flip. This belongs in `docs/contracts/terrain.md`.
11. **Placement preview.** Today the browser's preview ignores weather, planes, hidden units and mines. Proposal: the preview calls the core's own placement rule with what the player can see, so preview and server agree except where hidden information decides.
12. **Companion scope.** Whether the first-person companion is part of the first Unreal milestone.
13. **Upstream doc fixes.** Who corrects `DESIGN.md`, `CONTEXT.md` and `FPS-MVP.md` in ww2-rts, where 73 statements no longer match the code.
