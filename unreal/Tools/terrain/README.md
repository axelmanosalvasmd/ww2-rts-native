# Painted ground: tools

The native client paints the ground as the pinned reference does (`client/ground.js`): one canvas over the map, a few
pixels a cell, where every pixel blends the main and secondary materials of the four nearest cells at a noise-warped
position, with region tints, blast marks, light sweeps and a pale water line, then shell holes, trench cuts and contour
lines on top. The port is line for line, in the engine-free core of `Source/ThreeCrossroads/World/TCGroundPaint.h`
(namespace `TCGroundPaintCore`); `FTCGroundPaint` (same file, `TCGroundPaint.cpp`) feeds it from the start and
snapshot messages and turns it into textures, a fog texture and decal meshes. Nothing is hand-painted: the tiles are the
reference's own `buildTiles` output, the textures are `client/textures`, the numbers are the reference's.

| Reference (pin e4e579a) | Native |
| --- | --- |
| `client/ground.js` LOOK, BIOME_LOOKS, rnd, buildNoise, fieldCells, cellAttrs, biomeTint, the scar sampler, raster, overlays, shellHole, paint/tilePaint/queuePaint, createGround's pixels a cell | `TCGroundPaintCore::FGround` |
| `client/ground.js` buildTiles (textures tiled 3 x 3, shrunk by the canvas, tinted) | `World/TCGroundPaint.Tiles.inl`, written by `make-ground-tiles.mjs` from the browser's own output |
| `client/main.js` startGame and applyCells (the ground's part: grid, state, ground and object layers, materials, World rows, rememberLook's biome, setLevel; the decals' changed cells) | `FTCGroundPaint::ApplyStart`, `ApplySnapshot`, `FGround::Start`, `Apply` |
| `client/fog.js` per-cell look, quarter-second fade, LinearFilter; `client/surfaces.js` fowMist | `FTCGroundPaint` fog texture (1, 0.55, 0.32, bilinear), `MF_TCGroundPaint` (mist) |
| `client/rubble-decals.js`, `client/crater-decals.js` (slots, swap-remove, geometry, materials) | `TCGroundPaintCore::FDecalSet`, `RubbleQuad`, `CraterQuad`, `M_TCRubbleDecal`, `M_TCCraterDecal` |

## Files

| File | What it does |
| --- | --- |
| `ground-oracle.mjs` | The oracle. Runs the pinned `ground.js`, `rubble-decals.js`, `crater-decals.js`, `shared/sim.js` (heightAt cut out of the file and evaluated verbatim) under Node, unmodified, on all 33 authored maps as a start sends them, battle snapshots on 8 of them (and a small change after each, for the tile repaint), and a World Conquest sample made by the reference server code (generateWorldMap, createGame, worldMapFor, terrainFor, fogFor, damageCells, step). Writes the harness input, every raster (before the vector overlays), the cell attributes, the overlay drawing operations, the decals, the replays for the browser and `heights.json.gz` (every cell centre's heightAt and levelAt and 1,000 seeded points a map) |
| `canvas-recorder.mjs` | The browser APIs `ground.js` touches under Node: a canvas that keeps real pixels and records its vector drawing; images that never load (so the oracle paints the flat colours) |
| `client-replica.mjs` | `client/main.js` startGame and applyCells, the ground's part, copied line for line; shared by the oracle and the browser page |
| `harness/` | `run.sh <oracle> <scratch> [<tiles out>]`: builds `ground-harness.cpp` (C++17, no engine) on the core, replays every start and snapshot, compares every attribute and raster byte (Reference knowledge mode), checks that scrambling every unknown World cell changes nothing (Native mode), that the frame-budgeted queue paints the same pixels, and the textured canvas against the browser's; `compare.mjs` checks every overlay draw and decal vertex |
| `make-ground-tiles.mjs`, `tiles.html`, `tiles-page.mjs` | Headless Chromium runs the pinned `ground.js` with its textures and saves `buildTiles`' output for every material and tile size the game can pick (`--inl` writes `World/TCGroundPaint.Tiles.inl`); `--cases` also saves the browser's own textured canvas of oracle cases |
| `import-ground.py` | Editor Python: imports `client/textures` (ground pictures, crater picture, rubble atlas) and generates `MF_TCGroundPaint`, `M_TCGroundPaint`, `M_TCCraterDecal`, `M_TCRubbleDecal` and `TerrainProvenance.json` under `/Game/Terrain`; `--verify` checks them |
| `capture-prep.mjs`, `capture-ground.py` | An Unreal render of native output through the generated materials: the harness's Native canvas on a flat map, its fog mask (bilinear and, for comparison, nearest), its decals |
| `make-sheets.mjs` | Browser canvas, native canvas and their difference, side by side |

## The material interface (for the relief and anything else that draws ground)

`/Game/Terrain/Materials/MF_TCGroundPaint` outputs `Paint` (the painted ground, linear RGB, from absolute world position),
`FogDim` (1 seen, 0.55 explored, 0.32 never seen, bilinear between cell centres, with the drifting mist) and
`PaintFogged`. Its parameters (`GroundPaint`, `GroundFog`, `GroundMapSize` in map metres, `GroundFogOn`,
`GroundFogTime`, `GroundMist`) are set on any dynamic instance by `FTCGroundPaint::Bind`, which keeps it current
(textures, the mist's clock). `relief-material.js` starts from the painted ground as `diffuseColor`: `Paint` is that;
the fog overlay sits over the lit result, so `FogDim` multiplies the final base colour and emissive.
`M_TCGroundPaint` is the plain Lambert with base colour `PaintFogged`, usable on any ground mesh as it is.

## Regenerate and check

```
R=<this repository>; UE=~/Applications/UnrealEngine-5.8.3
D=<scratch>/three-deps   # three from the pin's package-lock (models README)
S=<scratch>
cd $R
# 1. the oracle (pure reference code under Node)
WW2_MODEL_DEPS=$D node --import ./unreal/Tools/models/register.mjs unreal/Tools/terrain/ground-oracle.mjs --out $S/oracle
# 2. the tiles, made by the reference in headless Chromium (writes the game's TCGroundPaint.Tiles.inl), and the
#    browser's own canvases of some cases
node unreal/Tools/terrain/make-ground-tiles.mjs --three $D --out $S/tiles --inl unreal/Source/ThreeCrossroads/World/TCGroundPaint.Tiles.inl \
  --oracle $S/oracle --cases default,crater-field,bocage,stalingrad-factory,world-huge-seed20261013,no-mans-land,the-polder
# 3. the engine-free port against both
unreal/Tools/terrain/harness/run.sh $S/oracle $S/harness $S/tiles
# 4. the assets, in an editor whose Content/Terrain is this repo's (a content-only host is enough), then verify
$UE/Engine/Binaries/Linux/UnrealEditor-Cmd <project> -run=pythonscript -unattended -nop4 -nosplash -nullrhi -script="$R/unreal/Tools/terrain/import-ground.py --work $S/import"
$UE/Engine/Binaries/Linux/UnrealEditor-Cmd <project> -run=pythonscript -unattended -nop4 -nosplash -nullrhi -script="$R/unreal/Tools/terrain/import-ground.py --work $S/import --verify $S/verify.json"
# 5. pictures: side by side sheets, and Unreal renders (the editor, not the commandlet: shaders finish on its ticks)
node unreal/Tools/terrain/make-sheets.mjs --oracle $S/oracle --tiles $S/tiles --harness $S/harness --out <dir> --cases default.1,world-huge-seed20261013.2
node unreal/Tools/terrain/capture-prep.mjs --oracle $S/oracle --harness $S/harness --case default --k 1 --out $S/cap/default-1
$UE/Engine/Binaries/Linux/UnrealEditor <host with ProceduralMeshComponent> -RenderOffscreen -unattended -nop4 -nosplash -NoSound \
  -ExecCmds="py $R/unreal/Tools/terrain/capture-ground.py --scene $S/cap/default-1/scene.json --out <dir> --prefix default-battle"
```

The harness's images need `--images <case>,...` (Native, Reference-mode and textured PPMs).

## Cook

`FTCGroundPaint` loads `/Game/Terrain/Materials/*` by path at run time. A packaged build needs
`+DirectoriesToAlwaysCook=(Path="/Game/Terrain")` in `Config/DefaultGame.ini`, and `tc-package-linux.sh verify` could
require `M_TCGroundPaint`, `M_TCCraterDecal`, `M_TCRubbleDecal`. Neither file is this slice's. The tiles are code
(`TCGroundPaint.Tiles.inl`), not assets, so nothing about them needs cooking.

## Not ported, or different on purpose

- **Unknown World cells.** The browser paints World's placeholder `.` cells (unknown ground) as open grass, with fields
  and all. Native mode does not read them: their pixels are a flat fog shade, no known pixel blends toward them, no
  blast mark, contour line, field or region tint reads them, and a field waits for its whole block to be known. The
  harness holds Reference mode to the browser byte for byte and Native mode to "scrambling unknown cells changes nothing".
- **Fog colour.** The browser mixes toward its overlay shade (10/255) by the overlay alpha; the native client dims by
  1, 0.55 and 0.32 (its existing convention, the same numbers the world pieces use). The quarter-second fade and the
  mist are kept; the World frontier's darkening of explored cells near unseen ones (`fog.js` FRONTIER) and the cells
  under each unit counting as seen until the next snapshot (`fog.js` lit) are not.
- **Graphics preset.** Always High (2048-pixel budget). The reference's Low halves the canvas.
- **Mipmaps** are made on the CPU (2 x 2 means in linear light) for the changed rectangles; the browser's
  `generateMipmap` is the GPU driver's.
- **Overlays** (shell holes, trench cuts, contour lines) use the reference's exact geometry and colours (`compare.mjs`)
  but a rasteriser of this file, not the browser canvas's anti-aliasing: on the overlay pixels the textured canvases
  differ by Delta E 0.2 to 0.9 on average.
