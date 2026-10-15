# Map objects and scenery: tools

The native client draws the map's pieces (houses, ruins, rubble, hedges, walls, sandbags, trenches, wire, traps, mines,
field hospitals, bridges) and its scenery props (trees, woods, bushes, grass, wheat, rocks, fences, haystacks, supply
heaps) with the reference's own models. The models come from `unreal/Tools/models` (`pieces/*`, `props/*`, `foliage/*`
in the export; `piece/*`, `prop/*`, `foliage/*` in `Content/Models/ModelCatalog.json`). Where each one stands, how big
it is, how it is turned and tinted is the reference's layout code, ported line for line:

| Reference (pin e4e579a) | Native |
| --- | --- |
| `client/structures.js` (every map builder, `put`, `compose`, KINDS) | `Source/ThreeCrossroads/World/TCStructures.h/.cpp` (`TCStructures`), with placement/types in `TCWorldObjects.h` (`TCWorldCore`) |
| `client/props.js`, `client/ground.js` (BIOME_LOOKS, fieldCells) | `Source/ThreeCrossroads/World/TCScatter.h` (`TCScatterCore`) |
| `client/main.js` hulks, applyGhosts, setNodes, houseAt | `FTCWorldObjects`, `FTCWorldObjectsView` (`TCWorldObjects.cpp`) |

The cores and structure builders are engine-free (`TC_WORLD_CORE_ONLY`), so they are checked against the reference outside Unreal too.

## Files

| File | What it does |
| --- | --- |
| `scenery-oracle.mjs` | The oracle: runs the pinned `structures.js` `buildStructures()` and `props.js` `candidates()` / `createProps()` under Node (unmodified, with the models tooling's inert browser shims) on 14 cases and writes every instance matrix and tint with the inputs: `fixtures/scenery-oracle.json.gz` |
| `fixtures/scenery-oracle.json.gz` | 14 cases: 7 authored maps as drawn (pieces and props, with supply nodes), ruins, damage stages 1 and 2, map walls as sandbags with mines, landmarks (mine head, factory, church), Low graphics, and a World map with region looks, partial discovery and railway cells (props). 65,457 piece instances, 8,085 candidates, 6,828 shown props |
| `harness/` | `run.sh <scratch>`: builds `scenery-harness.cpp` (C++17, no engine) on the two cores, feeds it the fixture's inputs (`to-input.mjs`) and compares everything with `compare.mjs` (2e-4 absolute plus 1e-5 relative; the reference stores Float32 matrices) |
| `make-world-materials.py` | Generates `/Game/World` (`Content/World`): the three world masters and one instance per imported material a piece, prop or foliage model uses, plus `WorldMaterials.json` (provenance). See its docstring |
| `compare/` | `shoot.mjs` + `village.html`/`village.mjs`: the reference half of a side by side. The pinned builders on a start map, `light.js` default mood, `camera.js` camera (fov 42, pitch 0.95), headless Chromium with SwiftShader |

## Why world materials

The imported materials reproduce each reference material, but:

- they read no per-instance colour, and `put()` / `place()` tint every instance;
- they sample no fog of war;
- the roof's tiles must keep their size on any roof (`roofShader`).

The world masters are the importer's formulas (`SURFACE_HLSL`, Lambert, the leaf's alpha test) plus those three things. Each instance copies its parameters from the imported instance it stands for.

**Per-instance custom data:**
- 0 to 2: the tint;
- 3: bark grey (`props.js` `c.bark`);
- 4 and 5: the roof's ridge and slope lengths over 1.6 m.

**Fog:** a one-texel-per-cell texture (`FogMap`, nearest) multiplies the colour by 1 in sight, 0.55 explored and 0.32 never seen, as `ATCTerrain` dims the ground. A fog flip rewrites texels, not instances.

## Regenerate and check

```
R=<this repository>; UE=~/Applications/UnrealEngine-5.8.3
D=<scratch>/three-deps   # three from the pin's package-lock (models README)
cd $R
# 1. the oracle (pure reference code under Node), then gzip it
WW2_MODEL_DEPS=$D node --import ./unreal/Tools/models/register.mjs unreal/Tools/world/scenery-oracle.mjs
gzip -9 -n -f unreal/Tools/world/fixtures/scenery-oracle.json
# 2. the engine-free port against it
unreal/Tools/world/harness/run.sh <scratch dir>
# 3. the world materials, in an editor whose Content is this repo's (after import-models.py), then verify
$UE/Engine/Binaries/Linux/UnrealEditor-Cmd <project> -run=pythonscript -EnablePlugins=PythonScriptPlugin -unattended -nop4 -nosplash -nullrhi \
  -script="$R/unreal/Tools/world/make-world-materials.py"
$UE/Engine/Binaries/Linux/UnrealEditor-Cmd <project> -run=pythonscript -EnablePlugins=PythonScriptPlugin -unattended -nop4 -nosplash -nullrhi \
  -script="$R/unreal/Tools/world/make-world-materials.py --verify <report.json>"
# 4. in-engine: ThreeCrossroads.World.PiecesOracle, .ScatterOracle (the fixture again, through the engine build) and
#    .ObjectsView (start, snapshot changes against a fresh start, the house pick, the HISM view with the world
#    materials, a wreck and a ghost)
$UE/Engine/Binaries/Linux/UnrealEditor-Cmd <project> -nullrhi -unattended -nosplash -nop4 -NoSound -ExecCmds="Automation RunTests ThreeCrossroads.World; Quit"
# 5. the reference half of a side by side (views: x,z,dist,yaw in reference metres and radians)
node unreal/Tools/world/compare/shoot.mjs --three $D --out <dir> --assets "map=default.json&views=67,119,45,0"
```

## Cook

`FTCWorldObjectsView` loads `/Game/World/Materials/Instances/*` by path at run time. A packaged build needs
`+DirectoriesToAlwaysCook=(Path="/Game/World")` in `Config/DefaultGame.ini`, next to `/Game/Models`. That file is
not this slice's; the line is left to the lead.

Graphics High/Low is connected through `FTCGraphics`, including structure detail, prop thinning and shadows. Trench builders read the carved relief floor once. The standalone harness links `World/TCStructures.cpp`; Terrain.StructureRules adds 72 source signatures and World.PiecesOracle retains its full instance comparison.

## Remaining review

- **Moving wrecks.** A wreck still flying (debris motion, `wrecks[i][6]`) is drawn where its row puts it, not interpolated (`client/debris.js` pose).
- **Ghost owner rings.** Ghosts are drawn without their owner ring. The ring is the unit renderer's.
