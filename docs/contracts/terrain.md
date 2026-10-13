# Terrain and recipient knowledge

Status: **v0 draft, technical answers recorded.** Author: Julio. Consumer reviewer: Axel.
Axel answered the drafts' technical questions on #5 (`axel-interface-response-004`, 2026-10-12); the answers are recorded below. Under `docs/agent-coordination.md` a comment is not an agreed interface, so the status stays "draft" until Axel approves a contract PR.
Baseline: reference ww2-rts `e4e579a`; native `main` after PR #7. Every rule below is as implemented at the pin; [`docs/parity/maps.md`](../parity/maps.md) has the evidence (MAP rows).

Julio owns world generation, map semantics and the gameplay queries; Axel builds rendered meshes from what the recipient has been told. Rendering never needs undiscovered world data.

## Simulation frame

- Positions are metres on the ground plane: `x` grows east (map column), `z` grows south (map row). Height is up. The origin is the north-west corner of the map. (MAP-01)
- A cell is 2 m square. Cell `(cx, cy)` has index `c = cy * w + cx` and centre `((cx + 0.5) * 2, (cy + 0.5) * 2)`. Points outside the map belong to no cell and count as solid for movement and sight. (MAP-01, MAP-03)
- Elevation is one integer level per cell, from -2 to 4; one level is 2.5 m. A map without `heights` is flat. Eyes are 1.6 m above the ground. (MAP-02)
- Angles are `atan2(dz, dx)`: 0 points east, and angles grow from east toward south.

## Mapping to Unreal

Unreal X = `x` × 100, Unreal Y = `z` × 100, Unreal Z = height × 100 (centimetres).

The simulation's axes (east, south, up) form a left-handed set, as Unreal's X, Y, Z do, so no axis flips. A simulation angle in radians becomes Unreal yaw in degrees by `yaw = angle × 180 / π`. Simulation angles grow from east toward south, the same direction Unreal yaw grows (from +X toward +Y), so there is no sign flip. A cell is 200 Unreal units and a level 250. The conversion lives in one function in the Unreal plugin; the core and the C interface stay in simulation units (`rust-unreal-abi.md`).

## Gameplay heights

Two height queries exist, and gameplay uses only these (MAP-56):

- `levelAt(x, z)`: the cell's integer level, a step function. Its height in metres is that level times 2.5. Sight, high ground and vision use it.
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

The [maintained fixture lane](../fixtures/terrain/README.md) now contains original-source captures for all 33 maps (661,799 cell centres and 33,000 query points) and a recorded World scouting history (17 receipts, 273,678 complete cell rows). Native equality and release export passed on macOS arm64 at `a81b4f2`, with an independent copied-binary and artifact audit. [The verification manifest](../fixtures/terrain/native-verification.json) records producing identity, counts, output hashes and refusal checks. Artifact publication and Unreal consumption remain separate. The fixtures distinguish raw integer `levelAt`, its derived height in metres and interpolated `heightAt`; they do not establish rendered-mesh parity or whole World tick support.

## Answered on #5 (2026-10-12)

1. **Mapping:** accepted as written, matching the Unreal client already built. The origin stays at the map's north-west corner.
2. **Conversion:** once, in the Unreal plugin. The core and the C interface stay in simulation metres; yaw goes from radians to degrees there.
3. **Rendered-ground limits:** the target for full terrain parity, not a gate the current placeholder terrain meets. The client's meshing and picking limitations stay listed until it meets them. Canonical height and query fixtures from the core are welcome.
