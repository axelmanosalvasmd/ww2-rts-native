# Client imports from `shared/` (seed for `docs/contracts/rust-unreal-abi.md`)

Source: `reference/ww2-rts` at ww2-rts commit e4e579a. All paths below are relative to that folder.

## Counts

| Scope | Import statements | Files | Distinct names |
| --- | --- | --- | --- |
| `client/**/*.js`, all `shared/` modules | 57 (plus 1 re-export) | 40 (plus `client/models/guns.js`) | 86 (plus `GUN_SLOTS`) |
| `client/**/*.js` from `shared/sim.js` | 35 | 35 | 56 |
| `server.js` | 9 | 1 | 31 (18 from `sim.js`) |
| `server/*.js` | 1 (`server/map-pings.js:1`, `CELL`) | 1 | 1 |
| HTML inline module scripts | 0 | 0 | 0 |

The plan's "56 names from `shared/sim.js` across 40 client files" is half right: 56 names is exact, but they come from 35 files. 40 is the number of client files that import from any `shared/` module. Ten imports are dead (imported, never used in that file): `BUILDABLE` in `hud.js`, and `UNIT_TYPES`, `SUPPORT_TYPES`, `BUILDABLE`, `canBuild`, `winVp`, `supCost`, `popCap`, `abCost`, `priceOf` in `main.js`. `BUILDABLE` is used by no client file, so the client really uses 55 `sim.js` names.

Per module (client): `sim.js` 56 names in 35 files, `world-layers.js` 5 in 5, `world-territories.js` 2 in 4, `weather.js` 6 in 3, `tech.js` 4 in 2, `formation.js` 3 in 2, `vehicle-motion.js` 3 in 1, `squad-men.js` 2 in 1 (plus `GUN_SLOTS` re-exported by `client/models/guns.js:1355`), `debris-motion.js` 2 in 1, `structures.js` 1 in 1, `scenarios.js` 1 in 1, `ai-schedule.js` 1 in 1. No client file imports `ai.js`, `ai-human.js`, `ai-mind.js`, `ai-view.js`, `grid.js`, `local-traffic.js`, `logistics.js`, `movement.js`, `navigation.js`, `projectiles.js`, `story.js`, `supply-convoys.js`, `tutorial.js`, `world-conquest.js`, `world-landforms.js`, `world-rails.js`, `world-roads.js` or `world-scenery.js` (but `fairness.js` reaches all of the sim and AI through `playMatch`).

How counted:

```
grep -rn "from '\.\./shared/\|from '\.\./\.\./shared/" client        # 58 lines: 57 imports + 1 re-export
grep -n "<script" client/*.html                                       # only src= module tags, no inline imports
```

plus a Node script that parses every `import { ... } from '...shared/...'` (multi-line aware, `as` aliases folded into the original name) over `client/**`, `server.js` and `server/*.js`. `client/es.js`, `client/report.js` and `client/symbols.js` mention `shared/` only in comments.

## Legend

- **Kind**: `C` constant or data table; `P` pure function of its arguments; `V` reads a game-state-shaped object (the client passes a view it builds from its snapshot); `W` creates or runs a whole game.
- **State**: `visible` means the client calls it with recipient-visible data only (its snapshot, its map, static tables). `hidden-risk` means the server version of the same call reads state the recipient does not have, so a port must take a client-built view and must not be wired to the server's world. `authoring` means editor-only use on a map the user holds in full; nothing hidden, but it needs the whole core.

## `shared/sim.js` (56 names, 35 client files)

| Name | Def | Kind | Client files: purpose | State |
| --- | --- | --- | --- | --- |
| `CELL` | `shared/sim.js:26` | C | 21 files (`apron`, `availability`, `camera`, `cover-preview`, `crater-decals`, `editor`, `engine-presentation`, `fairness`, `fog`, `fx`, `ground`, `lobby-view`, `main`, `map-view`, `markers`, `props`, `railways`, `relief`, `rubble-decals`, `structures`, `water`): metres per cell (2), cell and world coordinate conversion | visible |
| `TICK` | `:27` | C | `endgame` (match clock), `operative-view:381` (fire cooldown on 20 Hz ticks) | visible |
| `CFG` | `:28` | C | 13 files: `levelHeight` (relief, water, map), `pointRadius`, `reinforceRadius`, `fortBuilders`, `houses`, `paraRange`, `rail.*`, `catchupMax/Per`, `armies`, `terrainHp`, `stoneBridge`, `aid.radius`, `mine.detect` | visible |
| `MOVE`, `COVER`, `TRENCH` | `:171` | C | `cover-preview` (walkable and cover tests), `main` (`TRENCH` for trench snapping) | visible |
| `TERRAIN` | `:175` | C | `availability` (placement view flags), `cover-preview`, `editor` (map check grid), `main` | visible |
| `UNITS` | `:188` | C | 13 files: unit definitions for HUD, alerts, fog vision rings, sounds, markers, models, viewer, scenario editor | visible |
| `BUILDABLE` | `:372` | C | imported by `hud` and `main` but used in neither (dead import) | visible |
| `buildKinds` | `:378` | P | `availability:93`, `hud:351,602`: which buildings a mode allows | visible |
| `builderTypes` | `:379` | P | `availability:96,119`, `main:1287,1462`: builder squad types | visible |
| `isSkirmishBaseMode` | `:385` | V (`g.mode` only) | `availability:36,51`, `hud:351`, `main:752`: mode family | visible |
| `productionAccess` | `:387` | V (`g.mode`, `g.players[].team`) | `availability:53`, `main:938,1377,1462,1524`: may this slot use this building | visible |
| `productionBuildings` | `:388` | V (iterates `g.units`) | `availability:38,74`: facilities that can train a unit | visible (client passes snapshot units) |
| `priceOf` | `:391` | V (`g.mode`) | `availability:68,156`, `hud:599,624`: unit price (`main` imports it unused) | visible |
| `UNIT_TYPES` | `:392` | C | `hud:517` (`main` imports it unused) | visible |
| `canBuild` | `:394` | P | `hud:517,598`: faction filter (`main` imports it unused) | visible |
| `SUPPORT`, `SUPPORT_TYPES` | `:399`, `:414` | C | `SUPPORT` in `alerts`, `hud`, `main`, `map-view`, `viewer`; `SUPPORT_TYPES` in `hud` (`main` imports it unused) | visible |
| `FORTS` | `:423` | C | `availability`, `hud`, `main`: fortification table | visible |
| `lineFort` | `:449` | P | `main:1198`, `hud:353`: is this fort drawn as a line | visible |
| `ENTRENCH`, `ENTRENCH_TYPES` | `:472-473` | C | `hud`, `main`: entrenchment patterns | visible |
| `entrenchPlan` | `:477` | P (atan2, cos, sin, hypot, round, sort) | `main:1208`: segment preview; the server recomputes the same plan | visible |
| `segmentCost` | `:512` | P (round) | `main:1212`: preview cost | visible |
| `abCost` | `:520` | V (`g.mode`) | `availability:144`, `hud:444` (`main` imports it unused) | visible |
| `popUse` | `:532` | P | `availability:54,152,163`, `hud:674` | visible |
| `dropPop` | `:534` | P | `availability:90` | visible |
| `popCap` | `:537` | V (`g.mode`, `g.world`, `g.players`, `g.army`) | `availability:55,163`, `hud:675`: called as `s.world?.cap ?? popCap(s)`, so World uses the snapshot's cap (`main` imports it unused) | visible |
| `supCost` | `:549` | V (`g.mode`) | `availability:87`, `hud:158` (`main` imports it unused) | visible |
| `validateMap` | `:574` | P | `editor:60,96,162`, `structure-editor:44` | authoring |
| `MAX_PLAYERS` | `:623` | C | `editor` | visible |
| `POINT_KINDS` | `:624` | C | `editor` | visible |
| `spawnsFor` | `:626` | P | `editor:118,149` | authoring |
| `winVp` | `:628` | P | `endgame:77`, `hud:236` (`main` imports it unused) | visible |
| `createGame` | `:682` | W | `editor:128`: builds a game to check the map | authoring |
| `railYard` | `:1245` | P | `availability:179`: rail and station no-build cells from delivered cells | visible |
| `RAIL_DIRS` | `:1250` | C | `railways`: rail joint directions | visible |
| `teamSees` | `:1747` | V (iterates `g.units`; `los`, `sightMul(g)`) | `availability:197`: sight test for placement | hidden-risk (see note 2) |
| `OPERATIVE` | `:2042` | C | `operative-controls` (prediction), `operative-view` | visible |
| `OPERATIVE_WEAPONS` | `:2044` | C | `operative-view` (as `WEAPONS`) | visible |
| `houseKinds` | `:2274` | P | `structure-editor:13` | authoring |
| `houseKindsWhere` | `:2281` | P | `structures:343`: house types for drawing, per block | visible |
| `levelOf`, `levelChar` | `:2827-2828` | P | `levelOf` in 8 files (height chars to levels); `levelChar` in `editor:69`, `main:674` | visible |
| `startState` | `:2356` | P (`cellNoise`) | `ground:206`, `main:521`: a cell's look before the server mentions it | visible |
| `findPath` | `:3068` | W (A* over `g`) | `editor:168`: spawn-to-point reachability | authoring |
| `placementCheck` | `:3261` | V (`g.chars`, `flags`, `height`, `units`, `nodes`, `world`, `mines`, `structuralCells`) | `main:1208,1238,1297`: building, fort and entrenchment preview | hidden-risk (see note 1) |
| `unpackRuns` | `:4667` | P | `fog:29,30,72`: decode fog run strings | visible |
| `GATE_OPEN_FLAG`, `AUTO_FLAG`, `OPERATIVE_FLAG`, `RIDING_FLAG`, `CARGO_FLAG`, `CARGO_SHIFT` | `:5878-5882` | C | `autocast`, `availability`, `hud`, `main`: snapshot unit flag bits | visible |
| `UNIT_ROW` | `:5887` | C | `alerts`, `autocast`, `availability`: column index of each snapshot unit row field | visible |

Notes:

1. `placementCheck` on the server reads every unit (`stampOccupied`, `shared/sim.js:1012-1015`), the mine maps (`mineKnown`, `shared/sim.js:456`), gun pits, `initialTerrain` and bridge structures. The client calls it on a view it builds from its own snapshot (`client/availability.js:187-198`), so it sees only visible units and no mines. A preview can therefore say "ok" where the server answers "blocked". The view also has no `weather` field, so `sightMul` (`shared/weather.js:82`) returns 1 and the preview's sight ignores fog or snow. A Rust port must take the client view as input, never the authoritative world.
2. `teamSees` reads only the given team's units, and the client's own team is fully present in its snapshot, so the inputs are visible. It needs the client's known terrain (chars, heights, smoke) and should get the weather; the JS client view omits weather (note 1).
3. Formation and entrenchment results become order content: `facingSpots` output is the per-unit destination list the client sends (`client/main.js:1419,1441`). The Unreal client must produce them with the same arithmetic, which is why they belong in the ABI and not in Blueprint code.

## Other shared modules (client)

| Name | Module and def | Kind | Client files: purpose | State |
| --- | --- | --- | --- | --- |
| `playMatch` | `shared/ai-schedule.js:52` | W (whole sim plus AI, unseeded here) | `fairness:10`: web worker that plays N all-AI matches on an edited map | authoring |
| `DEBRIS_LIMITS`, `SECTION_MASS` | `shared/debris-motion.js:2-3` | C | `fx:805,809,829`: debris instance cap, fragment speed | visible |
| `SHAPES` | `shared/formation.js:15` | C | `hud:342`, `main:1460`: formation menu | visible |
| `slotSize` | `shared/formation.js:8` | P | `main:1419` | visible |
| `facingSpots` | `shared/formation.js:29` | P (cos, sin, sqrt, round in a sort comparator) | `main:1419`: move order destinations | visible (uses smoothed client positions) |
| `migrateScenario` | `shared/scenarios.js:7` | P | `editor:75` | authoring |
| `SLOTS`, `SLOT_SCALE` | `shared/squad-men.js:15,26` | C | `unit-models:484-497`: where each man stands | visible |
| `GUN_SLOTS` | `shared/squad-men.js:8` | C | re-exported by `models/guns.js:1355` | visible |
| `STRUCTURE_LIMITS` | `shared/structures.js:3` | C | `structure-editor:10` | authoring |
| `TECH`, `TIER_NAMES` | `shared/tech.js:13,19` | C | `hud:697-722`, `availability`: research panel text | visible |
| `techCost` | `shared/tech.js:22` | P | `availability:104`, `hud:717` (called with the snapshot's `s.tech`) | visible |
| `tierOf` | `shared/tech.js:10` | P | `availability:67,94` | visible |
| `isGroundVehicle`, `movementProfile`, `bodyRadius` | `shared/vehicle-motion.js:15-17` | P | `engine-presentation:138-141`: suspension and body pose | visible |
| `WEATHER`, `WEATHER_CHOICES` | `shared/weather.js:10,22` | C | `weather-view`: names, lobby choices | visible |
| `mapMood` | `shared/weather.js:37` | P | `atmosphere:515`, `moods:39` | visible |
| `mapWeather` | `shared/weather.js:47` | P | `weather-view:48`: the map's own plan for the lobby | visible |
| `roadMask` | `shared/weather.js:87` | P | `atmosphere:263`: puddle and mud mask | visible |
| `weatherEffects` | `shared/weather.js:122` | P | `weather-view:8` | visible |
| `BASE_SURFACES`, `MATERIALS` | `shared/world-layers.js:3,7` | C | `layer-edit`, `editor`, `structure-editor` | authoring |
| `composeWorldCell` | `shared/world-layers.js:16` | P | `availability:181,189`, `layer-edit`, `main:640,688`, `structure-editor` | visible |
| `migrateWorldMap` | `shared/world-layers.js:19` | P | `editor:76`, `layer-edit:4` | authoring |
| `worldLayers` | `shared/world-layers.js:53` | P | `availability:188`, `main:517`, `layer-edit`, `structure-editor` (on the map the server sent, already stripped by `mapForClient`) | visible |
| `territoryEdges` | `shared/world-territories.js:108` | P | `main:1825`, `map-view:183`, `world-regions:30`: region borders from runs | visible |
| `insideKnownRegion` | `shared/world-territories.js:132` | P | `alerts:225`: which team holds home | visible |

## Server imports (not part of the Unreal ABI; the Rust server owns them)

`server.js:13-23` imports from `shared/sim.js`: `createGame`, `step`, `command`, `snapshotFor`, `snapshotCache`, `unitDelta`, `terrainFor`, `fogFor`, `validateMap`, `spawnsFor`, `worldMapFor`, `joinOperative`, `operativeInput`, `OPERATIVE`, `UNITS`, `TICK`, `MAX_PLAYERS`, `FACTION_COUNT` (18 names); from `world-conquest.js` `generateWorldMap`, from `world-layers.js` `mapForClient`, from `weather.js` `WEATHER_CHOICES`, `weatherRow`, from `ai.js` `observe`, `resetAI`, `aiMemories`, `restoreAIMemories`, `AI_LEVEL_NAMES`, from `ai-schedule.js` `aiTick`, from `ai-human.js` `resetCommander`, from `command-feedback.js` `allowDeny`, from `story.js` `storyResult`. `server/map-pings.js:1` imports `CELL`. `server/room-tick.js`, `server/saves.js`, `server/shards.js` and `server/diag.js` import nothing from `shared/` (`room-tick.js` receives `step`, `aiTick`, `snapshotCache`, `snapshotFor` as arguments, `server/room-tick.js:5`). All of these run on the authoritative world.

## Mirrored rules

Client code that re-implements a shared rule instead of importing it, client-only rules that change which commands are sent, and the wire decoders the Unreal client also needs. Each entry gives the client `path:line`, the shared original and the known differences.

"Visible only" means the client copy reads only what the recipient was sent (its snapshot, the start map, delivered cells, its fog mask). Every line cited was read.

Search used: `grep -rniE "shared/|mirror|same as|in sync|server's|like the server|the server (does|uses|computes)|copy of|sim's" client`, then a read of each candidate area listed below.

Total: 51 entries (35 rule mirrors or client-only rules that decide gameplay inputs, 8 protocol decoders, 8 presentation-only items listed so nobody re-derives them as rules).

### 1. Cover and terrain queries (client/cover-preview.js)

The file says "Rules mirror shared/sim.js and are checked against the simulation in test.js" (cover-preview.js:3). The check is test.js:3572.

- **cover-preview.js:13-14 (`solid`, `wall`)**. Which cells give directional cover and which are house walls. Mirrors sim.js:2313 `SOLID` and sim.js:2314 `WALLS`. Same sets ('B', '#', 'R', 'H', 'K', 'Q'; 'B', 'K'). Visible only.
- **cover-preview.js:20 (`houseEdge`)**. A house cell a squad can garrison and shoot out of. Mirrors sim.js:2448 `entryCell` (open neighbour test). Difference: the client ignores which edge cells are already taken by other squads (`taken` in the sim). Visible only.
- **cover-preview.js:25 (`cellCover`)**. The cover the ground gives a squad, as NONE/OPEN/LIGHT/HEAVY. Mirrors sim.js:2306 `coverMul` with sim.js:2262 `inCover` and sim.js:2263 `inTrench`. Differences: four buckets instead of a multiplier; ignores cover quality (sim.js:2266 `coverQ`), trench facing (sim.js:2302 `trenchFaces`), trench settle time and house protection (`garrisonMul`). It reads the raw grid char, so a known mine cell 'N' (TERRAIN.N = 0) reads as open ground, while the sim reads the cell under the mine (sim.js:2297 `physicalWorldCell`). Visible only.
- **cover-preview.js:38 (`solidToward`)**. Terrain half of directional cover toward a shooter. Mirrors sim.js:2317 `coverBehind` (terrain loop) and sim.js:2339 `behindCover` (> 0.4). Difference, recorded at cover-preview.js:32-37: the sim uses a continuous quality (coverQ from cell hit points); the client only has the 2-bit damage stage, so it uses fixed cut-offs (d < 2.5 m, or d < 3.3 m when stage < 2). Visible only.
- **cover-preview.js:50, 53 (`vehicleCovers`, `vehicleToward`)**. Vehicle and wreck half of directional cover. Mirrors sim.js:2331-2337 (`hull` over live heavy units and `g.wrecks`). Differences: positions are snapshot rows rounded to 0.1 m (sim.js:5877 `rounded`); at most 48 vehicles (cover-preview.js:73). Hidden-state gap: the sim counts every vehicle in range, including ones the viewer cannot see; the client can count only delivered units and wrecks (cover-preview.js:161-162).

### 2. Placement, entrenchment and dig previews

- **availability.js:187 (`placementState`)**. Builds a sim-shaped game object from the snapshot so the imported `placementCheck` and `teamSees` can run in the browser. It mirrors the server's game state fields: `flags` from TERRAIN (sim.js:1640 builds the same per-cell flags for a player view), `height` from the patched map heights (main.js:671 `setLevel` keeps them current), `nodes` with a depot linked when a depot row sits within 1 m (availability.js:194) and the node cell rebuilt as the inverse of sim.js:1019 `footCenter`. Differences: the object has no `weather`, so `sightMul` (shared/weather.js:82) returns 1 and client sight ignores weather; unit objects from `snapshotUnits` carry no `air` field, so `teamSees` (sim.js:1747) treats a plane as a ground eye needing line of sight; positions are rounded to 0.1 m. Hidden-state gap: `stampOccupied` (sim.js:1012) blocks a footprint on any unit standing in it; the preview only knows delivered units, so a hidden (camouflaged, flag 256) enemy in a visible cell passes the preview and fails on the server. Tested in test-world-conquest.js:123 and test-world-territories.js.
- **availability.js:169 (`rememberPlacementTerrain`)**. The `initialTerrain` and `railYard` baseline that `placementCheck` reads. Mirrors the server's initial-terrain record (`g.initialTerrain`); for World maps it grows only from delivered cells. Visible only.
- **main.js:1204 (`entrenchSegments`)**. Which segments of a mass entrenchment can be dug: `entrenchPlan` then `placementCheck` per segment, from the centroid of non-retreating diggers. Mirrors sim.js:3668-3674 (the `entrench` command). Same algorithm with imported functions; differs only through the `placementState` gaps above.
- **main.js:1211 (`entrenchSummary`)**. Cost and how many segments start now. Mirrors pay-as-you-go digging in sim.js:3317 `takeDigJob` and sim.js:3323 (`segmentCost`). Difference: the client charges the first N segments in plan order; the server lets each digger take the nearest of the next `crew` segments and re-checks placement first. Display only.
- **main.js:1383 (`defaultDir`)**. Default facing of a dig (across the nearest digger's approach) and of a support call (away from home). Mirrors sim.js:3641 (dig) and sim.js:3687 (support, from `p.spawn`). The browser always sends `dir`, so the server default only runs for other senders.
- **main.js:1328 (`nearestDigger`)**. Client-only rule: which selected squad receives a `dig` order (the sim uses `ids[0]`). The Unreal client must make the same choice to send the same command.
- **main.js:1418 (`formation`), main.js:1412 (`rearRank`), main.js:1425 (`snapToTrench`)**. Client-only rules that compute the move spots sent in every `move`/`amove` order: imported `facingSpots`/`slotSize`, mortars, rockets and medics marked `back`, infantry spots snapped to a free trench cell within 3 m, then clamped to [1, map size - 1]. The server's own `spreadFacing` (sim.js:4005) calls `facingSpots` without these steps. Visible only (terrain grid, own units).

### 3. Availability, prices, population and mode tests

- **availability.js:46 (`availability`)**. Pre-checks every order the HUD offers. Mirrors the validation branches of sim.js:3408 `command`: buy sim.js:3781, support sim.js:3680, build sim.js:3689, tech sim.js:3723, dig sim.js:3635, entrench sim.js:3651, cancelProduction sim.js:3737, stance sim.js:3574, cover sim.js:3580, ability sim.js:3602. Differences: it returns English sentences, not reason codes; support does not check the click spot (`teamSees`, `paraField` range), only that a finished Airfield exists in World mode (availability.js:89); the Classic queue limit 5 is a literal (availability.js:80, 161; sim.js:3808 and sim.js:1367). Visible only (own snapshot: mp, fuel, mun, sup, tech, own rows, queues). Tested in test-skirmish-client.js:6-8.
- **availability.js:41 (`resources`)**. Resource checks for each order. Mirrors the `p.mp < price.mp`, fuel and mun checks in sim.js:3792 (buy), sim.js:3734 (tech) and sim.js:3682 (support). Same comparisons.
- **availability.js:154 (`buyCount`)**. Client-only rule: how many units one Shift purchase buys. Repeats the buy limits (money, fuel, army limit, `def.max`, queue room of 5).
- **availability.js:152 (`popTotal`) and hud.js:674 (pop in `update`)**. Army size. Mirrors sim.js:535 `popOf`. Difference: the client leaves out support drops still in the air (`g.strikes` with `!live`, counted by `dropPop`), so it can show a lower count than the server uses. Visible to the owner in principle, but not sent.
- **availability.js:51, availability.js:156, hud.js:680, main.js:81 (`classicMode`), hud.js:715**. Inline copies of sim.js:384 `isConstructionMode` (`mode.kind` is 'classic' or 'world'). main.js:81 decides from the lobby mode or `map.world`; hud.js:715 decides from `s.mun !== undefined`. Neither imports the shared function.
- **availability.js:6, 22 (`DENY_SENTENCES`, `denySentence`)**. Text for the server's reason codes (for example sim.js:3672 'notVisible', sim.js:3683 'unseen', 'pop', 'airfield'). Client-only table, but the code list is the contract the Unreal client needs.
- **main.js:1356 (`railers`)**. Which selected units may ride the train. Mirrors sim.js:3547. Difference: the client drops structures and rail units; the server drops air, naval and units in forced withdrawal (`logistics.forced`).
- **main.js:1387 (`PRIORITY`), main.js:1388 (`fKeyType`)**. Client-only rule: which ability the F key fires.

### 4. Squad men and the first-person operative

- **main.js:948 (men alive)**. `Math.ceil(hp * v.models.length / (def.models * def.hpPer))` re-implements shared/squad-men.js:28 `menDrawn` instead of importing it. Same result because unit-models.js:484 builds one man per `SLOTS[type]` entry.
- **main.js:952-958 (shot man trades slots)**. Client-only rule that keeps the man an operative hit (the `m` index sent at sim.js:2095-2096, counted by `menAt` at sim.js:2069) as the one who falls.
- **main.js:828 (`manTrench`), main.js:851 (`seatTrench`)**. Client-only: men of a trenched squad line nearby 'T' cells, four per cell, claimed in snapshot order. The sim keeps a trenched squad as one 1.2 m body (sim.js:2060, sim.js:2071), so hits never use these spots.
- **squad-motion.js:15 (`moveSquad`)**. Visual followers. Each man eases toward his slot at 7 + 4 x seed per second. shared/squad-men.js:31 `menAt` approximates the trail as speed / 9 behind the slot (squad-men.js:29-30 says so). Hit spots and drawn men differ during turns and acceleration. Presentation, but it sets what "the soldier you see is the soldier you hit" means.
- **operative-controls.js:15 (`predictStep`)**. Local movement prediction. Mirrors sim.js:2109 `stepOperative` (lines 2116-2130). Differences: no wall check or axis slide (`operativeCanStand`, sim.js:2133-2134); operative-view.js:425 steps at 1/120 s while the server steps at TICK (1/20 s), so easing and jump arcs integrate differently. test-operative-client.mjs:23 checks only at TICK steps, open ground, 0.02 m tolerance. Reconciled by operative-view.js:356.
- **operative-view.js:374 (`spread`)**. Mirrors the spread formula at sim.js:2054. Difference: the client uses predicted velocity; the server uses `u.moveSpeed`, the actual displacement over the tick (sim.js:2135).
- **operative-view.js:388-398 (bullet guess in `shoot`)**. Mirrors the ray at sim.js:2077-2086. Differences: steps of 0.25 m from 0.5 m (server 0.2 m from 0.4 m); bodies of 0.9 m (squad) or 1.6 m, 1.9 m tall (server: 0.45 m per drawn man, 1.2 m, `def.radius`; tops 1.85, 3 or 5 m); no sight or map-edge checks; ground from the client relief. Cosmetic guess; the server decides.
- **operative-view.js:381, 88, 93, 370**. Fire cooldown rounded up to whole ticks, swap and reload timers, and the local magazine count. Mirror sim.js:2113 (`cooldown - TICK`), sim.js:2140-2142 (swap and reload) and sim.js:2051 (`mags--`).
- **operative-controls.js:6 (`lookDelta`)**. Client-only input: yaw wrap and pitch clamp at 1.45, the same bound sim.js:2026 `operativeInput` enforces.

### 5. Terrain height and fog

- **relief.js:460 (`hAt`)**. Ground height under a point. The sim has two: sim.js:2832 `heightAt` (bilinear between cell centres, used for operative eyes and bullets) and sim.js:2830 `levelAt` (step, used for sight lines at sim.js:2842). The client mesh keeps cell-centre heights (relief.js:1) but ramps only between connected neighbours, cuts cliffs, sinks trench cells by 0.9 m (relief.js:41, "the sim's cover and sight lines ignore it") and samples detail triangles. Used by the operative camera and bullet guess, so a first-person view can disagree with the server's ground near cliffs and trenches.
- **fog.js:84-95 (`state.snapshot`, lit cells)**. Client-only rule: the cells under every delivered ground unit count as seen until the next snapshot, to cover the server's 5 Hz vision pass (fog.js:67-69). A building's cells use `Math.round(x / CELL - size / 2)`, the footprint origin at sim.js:3290 in `placementCheck`. Visible only.

### 6. Projectiles, debris and the tick rate

- **projectiles.js:38-39 (`frame`)**. Flight position and velocity. Re-implements shared/projectiles.js:22 `flightAt` and shared/projectiles.js:26 `velocityAt` on the wire fields (`p.v`, `p.time` instead of `vx`/`vy`/`vz`, `stamp`). Same ballistic formula; extrapolates at most 0.15 s past the snapshot (projectiles.js:35).
- **20 ticks per second written as a literal**. Mirrors sim.js:27 `TICK = 1 / 20` instead of importing it: projectiles.js:14, debris.js:7, debris.js:20, alerts.js:178, main.js:1032, main.js:2008, fairness.js:18, report.js:99. (endgame.js:71 and operative-view.js import TICK.)

### 7. Logistics display

- **logistics.js:52 (`logisticsIndicator`)**. "Low supplies" thresholds written as literals: ammo < 0.25, provisions < 30, fuel < 45. Mirrors shared/logistics.js:117 `shortageState` (LOGISTICS.low 0.25 x provisions 120 = 30). Difference: the client also flags fuel below 45 s (0.25 x 180), which `shortageState` does not. Visible only (owner rows).
- **logistics.js:104 (alert throttle)**. 20 s, the same value as `LOGISTICS.alertSeconds` (shared/logistics.js:15), written as a literal.
- **logistics.js:42 (`storeForUnit`)**. Client-only join: a store row belongs to a unit when owner matches and it sits within 0.5 m.
- **logistics.js:61 (`overlayItems`)**. Client-only rule: a truck route is drawn only if every 1 m sample along it is seen or explored. Keeps hidden ground hidden.

### 8. Economy and victory display

- **endgame.js:86-88 (catch-up bonus)**. Mirrors the catch-up term of sim.js:5752 (`Math.min(CFG.catchupMax, (lead - teamVp) / CFG.catchupPer) * income`). Difference: the server zeroes income for `pl.away || pl.out`; the client hides the bonus only when its own seat is offline, and shows it for one trailing team.
- **endgame.js:71-83 (victory ETA)**. Client-only: VP rate over a sliding window, with imported `winVp`.

### 9. Client-only rules that generate commands

These have no shared original but change which commands reach the server, so the Unreal client must reproduce them for the same orders to be sent.

- **orders.js:9 (`kind`) and orders.js:22 (`dispatch`)**. Right-click to command mapping: supply, escort, board, assist, entrench (join), attack, house (garrison for infantry that garrisons, `fireat` for guns with `shellTerrain` or `salvo`, move for the rest), move or attack-move, rally.
- **autocast.js:24 (`adopt`)**. Sends `autocast` on or off for each own unit seen for the first time, from per-type preferences in localStorage (kept apart for Classic). Changes match behaviour from browser storage.

### 10. Protocol decoders (wire contract, not rules)

- **world-state.js:11 (`apply`)**. Keeps the held row set and checks `held` = [count, xor of ids]. Mirrors sim.js:5951 `unitDelta`. Same count and xor.
- **availability.js:29 (`snapshotUnits`)**. Decodes rows with `UNIT_ROW` (sim.js:5887); `built` defaults to 1; queues from `s.queues`.
- **Cell state bits**: ground.js:206, 225, 229; main.js:691; cover-preview.js:129; structures.js:967. Decode sim.js:2352 `packState` (wear bits 0-1, burnt bit 2, damage stage bits 3-4, shelling bits 5-6).
- **Unit row flag bits by literal mask**: availability.js:98 (1 retreating), hud.js:272 and main.js:967 (256 hidden/camo), hud.js:282 (32 garrisoned, bits 20-22 house type), main.js:1930 (512 plane not drawn), hud.js:45 (8192 auto-retreat). Encoded at sim.js:5883 `unitFlags` and sim.js:3334 `stanceBits`; sim.js:5903 masks what other players get.
- **fog.js:72-73**. Fog deltas toggle cells (`seen[c] ^= 1`) over runs from imported `unpackRuns`.

### 11. Presentation-only items (no rule to port, listed to avoid re-deriving them)

- **main.js:1923-1926**. Unit positions ease toward the latest snapshot with k = 1 - exp(-dt x 1000 / snapshotGap); there is no client-side vehicle or infantry movement prediction (`vehicleStep`, shared/vehicle-motion.js:87, is not used by the client).
- **debris.js:23-30**. Falling debris interpolates linearly between delivered poses; shared/debris-motion.js:29 `stepDebris` runs only on the server.
- **model-motion.js:3-4**. Its own LAND and SEA vehicle type lists for hull sway; overlaps shared/vehicle-motion.js:15 `isGroundVehicle` but includes 'lorry'.
- **world-regions.js:30**. Border vertices use a literal `* 2` for CELL (sim.js:26).
- **ground.js:28 (`BIOME_LOOKS`)**. Indexed in the order of shared/world-layers.js:6 `BIOMES`; must stay in that order.
- **engine-presentation.js:138-141**. Uses imported `movementProfile` and `bodyRadius` for the terrain pose only.

### Out of scope, noted once

shared/sim.js:2055 (`fireOperative`) draws spread with `Math.random()`, outside the seeded stream. That belongs to the determinism hazards list (item 3).

One more difference, from the import review: the client's `facingSpots` input uses each unit's eased display position (`client/main.js:1419`, positions eased at `client/main.js:1925`), while the server's `spreadFacing` (`shared/sim.js:4005-4006`) uses authoritative positions. The spots the client sends therefore depend on render timing, so a browser and an Unreal client given the same click can send different move orders even with identical math.
