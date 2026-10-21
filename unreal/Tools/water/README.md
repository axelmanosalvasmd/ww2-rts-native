# Water and the land past the map edge: tools

The native client draws the reference's water and apron with the reference's own code, ported line for line:

| Reference (pin e4e579a) | Native |
| --- | --- |
| `client/water-levels.js`, `client/water.js` (levels, mask, flow, mesh, edge strips, change rules, noise table) | `Source/ThreeCrossroads/World/TCWater.h` (`TCWaterCore`, engine-free), `TCWater.cpp` (`FTCWater`: the mesh, the mask and noise textures, the fog texture) |
| `client/apron.js`, and the parts of `client/ground.js` it reads (noise tables, LOOK, BIOME_LOOKS, field blocks, cell materials) and `shared/sim.js` `startState` | `World/TCApron.h` (`TCApronCore`, engine-free), `TCApron.cpp` (`FTCApron`: the ring mesh, the cell, noise and tile textures) |
| water.js `FRAG_*`, apron.js `FRAG_COLOR`, surfaces.js `fogOverlayShader` | `Content/Water/M_TCWater`, `M_TCApron`, made by `make-water-materials.py` |

Both cores compile alone (`TC_WATER_CORE_ONLY`, `TC_APRON_CORE_ONLY`), so the harness checks them outside Unreal.

## Files

| File | What it does |
| --- | --- |
| `oracle.mjs` | The oracle: runs the pinned `createWater`, `createApron`, `createRelief`, `createGround` and, for World Conquest, `createGame`/`worldMapFor`/`terrainFor` under Node, unmodified (the models tooling's inert shims), on all 33 maps and a World Conquest start (seed 17) with steps: a crater beside water, the crater flooding into a ford, a destroyed bridge, a dented river bed, a dent beside a raised span (World: the first water found, then a scouting band). Writes per case the inputs (`input.txt`: cells, levels, frozen levels, knowledge, states, every ground height the reference code asked for) and what the reference built (`*.bin`), `summary.json` (sources' hashes, terrain.md checks) and both noise textures |
| `harness/` | `run.sh <oracle dir>`: builds `water-harness.cpp` (C++17, no engine) on the two cores, rebuilds every case from `input.txt` and compares with `compare.mjs` (positions 1e-4 m, flow and normals 1e-5, indices, masks, cell texels and noise bytes exact) |
| `make-water-materials.py` | Generates `/Game/Water` (`M_TCWater`, `M_TCApron`) and `Content/Water/WaterMaterials.json` (sources and HLSL hashes). `--verify <json>` loads and lists them |
| `shots/reference-shots.mjs` | The reference half of a side by side: the pinned browser client itself in headless Chromium against the isolated server, one AI, clear weather, posed through `window.__game.cam` at the native `-TCWaterShots` poses, canvas only |
| `shots/compose.mjs`, `shots/png.mjs` | Reference-left, native-right sheets (each frame halved) and the share of near-black pixels per frame; a dependency-free PNG reader and writer |

## Regenerate and check

```
R=<this repository>; UE=~/Applications/UnrealEngine-5.8.3
D=<scratch>/three-deps   # three from the pin's package-lock (models README)
cd $R
# 1. the oracle (pure reference code under Node) into a scratch folder (about 20 s, 0.5 GB)
WW2_MODEL_DEPS=$D node --import ./unreal/Tools/models/register.mjs unreal/Tools/water/oracle.mjs --out <scratch>/oracle
# 2. the engine-free port against it
unreal/Tools/water/harness/run.sh <scratch>/oracle
# 3. the materials, in an editor whose Content is this repo's
$UE/Engine/Binaries/Linux/UnrealEditor-Cmd $R/unreal/ThreeCrossroads.uproject -run=pythonscript -EnablePlugins=PythonScriptPlugin \
  -unattended -nop4 -nosplash -nullrhi -script="$R/unreal/Tools/water/make-water-materials.py"
# 4. in-engine: ThreeCrossroads.World.WaterReference (test-world.js's water checks) and ThreeCrossroads.World.ApronShape
$UE/Engine/Binaries/Linux/UnrealEditor-Cmd $R/unreal/ThreeCrossroads.uproject -nullrhi -unattended -nosplash -nop4 -NoSound \
  -ExecCmds="Automation RunTests ThreeCrossroads.World; Quit"
# 5. captures: native with -TCWaterShots=<dir> -TCHostWeather=clear (game mode glue), reference with
node unreal/Tools/water/shots/reference-shots.mjs --server http://127.0.0.1:<port> --map river-towns --out <dir>
node unreal/Tools/water/shots/compose.mjs <reference dir> <native dir> <sheets dir> <prefix>
```

The commandlet saves the materials without compiling their shaders; they compile the first time a renderer loads them
(a game run or the editor).

## Wiring (the lead's)

`FTCWater` and `FTCApron` are fed next to `ATCTerrain`: `Attach(World)` once, `GroundHeight` set to the drawn ground
(`ATCTerrain::HeightAt`), `Apron.Objects = &WorldObjects` (cell states and physical cells), `ResetFrom(State)` after the
terrain's, `MarkCells(State, DirtyCells, FogDirtyCells)` before the terrain's, `Flush(...)` after the terrain's Flush,
`Detach()` in EndPlay. `FTCApron::SetTiles` takes the paint's tiles (six textures and their metres; say whether they
decode sRGB and whether they are raw photos or ground.js's already tinted tiles). `FogGamma` (both) is 1: the fog
multiplier dims linear colour as the world pieces' FogMap does; 2.2 matches a ground that scales sRGB bytes.

## Integration notes and remaining review

- **Frozen levels of later-discovered World cells.** The reference freezes every cell's level when the water is made, so a
  World cell discovered later keeps the start map's placeholder level 0. The native adapter now does the same, without
  reading hidden levels. Terrain.WaterDiscovery checks the actual adapter, in addition to the standalone source oracle.
- **gfx.low.** The native Graphics preset connects the material's `Low` switch at creation and on live changes, freezing
  the drift as the reference does. World.GraphicsSwitch protects that connection.
- **The apron's mist and cloud shade.** `fowMist` (surfaces.js) and the cloud shade overlay over the near rows
  (`overlayGeometry`, `CloudCount` in the core) are supplied by the integrated atmosphere. Weather visual acceptance
  remains tracked separately in the parity table.
- **Textures.** Until `SetTiles`, the apron uses the reference's flat colours (its look before the textures load).
