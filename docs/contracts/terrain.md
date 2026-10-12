# Terrain and recipient knowledge

Status: **v0 draft, pending agreement.** Author: Julio. Consumer reviewer: Axel.
Baseline: reference ww2-rts `e4e579a`; native `main` after PR #7. Every rule below is as implemented at the pin; [`docs/parity/maps.md`](../parity/maps.md) has the evidence (MAP rows).

Julio owns world generation, map semantics and the gameplay queries; Axel builds rendered meshes from what the recipient has been told. Rendering never needs undiscovered world data.

## Simulation frame

- Positions are metres on the ground plane: `x` grows east (map column), `z` grows south (map row). Height is up. The origin is the north-west corner of the map. (MAP-01)
- A cell is 2 m square. Cell `(cx, cy)` has index `c = cy * w + cx` and centre `((cx + 0.5) * 2, (cy + 0.5) * 2)`. Points outside the map belong to no cell and count as solid for movement and sight. (MAP-01, MAP-03)
- Elevation is one integer level per cell, from -2 to 4; one level is 2.5 m. A map without `heights` is flat. Eyes are 1.6 m above the ground. (MAP-02)
- Angles are `atan2(dz, dx)`: 0 points east, and angles grow from east toward south.

## Mapping to Unreal (proposal)

Unreal X = `x` × 100, Unreal Y = `z` × 100, Unreal Z = height × 100 (centimetres).

The simulation's axes (east, south, up) form a left-handed set, as Unreal's X, Y, Z do, so no axis flips. A simulation angle in radians becomes Unreal yaw in degrees by `yaw = angle × 180 / π`, because both turn from the first axis toward the second. A cell is 200 Unreal units and a level 250. The conversion lives in one function in the Unreal plugin; the core and the C interface stay in simulation units (`rust-unreal-abi.md`).

## Gameplay heights

Two height queries exist, and gameplay uses only these (MAP-56):

- `levelAt(x, z)`: the cell's level times 2.5 m, a step function. Sight, high ground and vision use it.
- `heightAt(x, z)`: bilinear interpolation between cell centres. Slopes and the companion's footing use it.

The core exposes both on the client view, so overlays, picking checks and the companion agree with the server.

## Rendered ground

The drawn ground may be smoother than the step grid, within these limits taken from the browser client (MAP-57, checked there on all 33 maps):

- every cell centre within 0.1 m of its level × 2.5 m, except banks and trenches;
- trench cells 0.9 m lower; roads 0.1 m lower;
- each body of water at a line from the frozen map levels plus 0.06 m; fords 0.15 m below that line; channels 0.42 m below it plus 0.38 m per row inward;
- bridges at deck height;
- steps of two or more levels drawn as vertical cliffs.

The fixture for this: a table of every cell centre's gameplay height on all 33 maps, produced by the core, against which the Unreal mesh is sampled.

## What a recipient receives

- **Authored maps.** `start.map` is the public copy of the map file (mines and triggers stripped). Later changes arrive as cell rows when the recipient's team observes them. (MAP-24, MAP-104)
- **World Conquest.** `start.map` writes unknown cells as open ground at level 0; `start.cells` lists every cell the team knows. A client must treat a cell as known only if it arrived in a cell row (MAP limitation). Regions appear once their objective cell has been seen or the team owns or owned them; a region's explored part arrives as runs `[row, x0, x1)`. (MAP-58, MAP-101)
- **Cell rows** are `[c, ch, level, state, data]`: index, terrain character (a hidden enemy mine reads as the cell beneath it), level, packed state (wear, burnt, damage stage, shelling) and a data object with ground, object, material and section fields (field reference: `docs/parity/protocol-messages.md`, section 5.6).
- **Memory.** Out-of-sight changes stay hidden: a client keeps the last state it saw, and rescouting delivers the current one (MAP-100). The match end reveals the whole map (MAP-103).

## Placement, picking and line of sight

- Unreal picks against its rendered mesh, converts the hit to simulation coordinates, and asks the core's client view whether a placement is valid there.
- The preview can only use what the recipient knows, so the server may still refuse a placement because of a hidden unit or mine. The refusal reason names no hidden detail (issue #4, section 3).
- Line of sight, cover and obstacles are gameplay rules in the core (MAP-50, MAP-51). The client draws fog from the fog the server sends, never from its own sight computation.

## Map files

v0 keeps the map file format at the pin (MAP-04 to MAP-19): the row format, the layered `worldVersion: 2` format and its migration. Changing the format is a later contract change. Whether Unreal gets a map editor is open (`rust-unreal-abi.md`, question 5).

## Fixtures

Pending, produced by the core once it loads maps: cell-centre heights for all 33 maps; `heightAt` and `levelAt` at 1,000 points per map; the cell rows of a recorded World Conquest scouting run.

## Open questions for Axel

1. The mapping above: accepted, or does Unreal want another origin (for example the map centre at the world origin)?
2. Conversion in the Unreal plugin (proposed) or in the C interface?
3. Do the rendered-ground limits above work for Unreal's terrain, given that the ground changes per cell at run time (craters, trenches, bridges)?
