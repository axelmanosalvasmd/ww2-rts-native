# Reference model export

`export-reference-models.mjs` writes every model the pinned reference client (`reference/ww2-rts`) renders as
glTF 2.0 binaries, by running the reference's own builders under Node. Nothing is re-modelled: positions, normals,
vertex colors, indices, hierarchy, pivots and the shader attributes come from the same code the browser runs.

```
node unreal/Tools/models/export-reference-models.mjs --deps <scratch dir> [--out unreal/Saved/ModelExport] [--jobs 4]
node unreal/Tools/models/validate-models.mjs [--out unreal/Saved/ModelExport] [--validator <dir with npm gltf-validator>] [--report FILE]
WW2_MODEL_DEPS=<scratch dir> node --import ./unreal/Tools/models/register.mjs unreal/Tools/models/roundtrip-check.mjs --reference reference/ww2-rts --out unreal/Saved/ModelExport [--look slot1]
```

`--deps` is a scratch directory for the pin's own three.js. The exporter copies the pin's `package.json` and
`package-lock.json` there and runs `npm ci --ignore-scripts` when `node_modules/three` is missing, then checks the
installed version against the lock. `reference/ww2-rts` is never written to; the exporter refuses to run if its tree
is dirty. Output goes to `unreal/Saved/ModelExport` (not Content): Unreal import is a separate step.

## Files

| File | Role |
| --- | --- |
| `export-reference-models.mjs` | orchestrator: pin check, deps, workers, texture and source copies, `manifest.json` |
| `build-worker.mjs` | one Node process per look: builds every asset, dumps colors, or writes the .glb files |
| `catalog.mjs` | the asset list: which reference function builds what, with which arguments |
| `extract.mjs` | three.js object tree to plain arrays (no geometry changes) |
| `compose.mjs` | arrays to .glb: attribute mapping, morph targets, materials, bounds |
| `authored-rig.mjs` | the Blender-baked infantry payload as a skinned rest-pose mesh |
| `gltf-writer.mjs` | deterministic GLB writer and reader |
| `layers.mjs` | the model texture layer table (`_MATID` index to texture file) |
| `shims.mjs`, `register.mjs`, `three-resolve.mjs` | the browser shims and the `three` resolve hook |
| `validate-models.mjs` | Khronos glTF-Validator (optional) plus own structural checks against the manifest |
| `roundtrip-check.mjs` | reads the files back and compares them with a fresh reference build in another player color |

## Output

| Path | Built by |
| --- | --- |
| `units/<type>/<type>-f<faction>.glb` | `client/unit-models.js buildModel()` for every non-air type in `shared/sim.js UNITS`, factions 0 USA, 1 Germany, 2 USSR, 3 UK |
| `units/<type>/<type>-riders-f<faction>.glb` | `unit-models.js setRiders(v, n, look, faction)` for n = 1, 2, 3 squads aboard (tank, medium, tankdestroyer, tiger, churchill): nodes `riders_1..3`, show exactly one |
| `planes/<role>/<role>-f<faction>.glb` | `client/aircraft.js createAviation().preview()` (the plane units, support runs and the viewer share it), roles fighter, attacker, bomber, transport |
| `planes/bomb.glb` | the bomb `aircraft.js` hangs under dive-bombing planes |
| `structures/airfield/...` | `aircraft.js buildAirfield()` |
| `structures/hq-camp*/...`, `structures/hq-sandbag-ring/...` | `client/structures.js hqCamp(look, tent)` and `sandbagRing(CFG.reinforceRadius + 0.8)` as `client/main.js buildHQ()` calls them |
| `pieces/<kind>.glb` | each `client/structures.js` KINDS entry (map pieces), as drawn by `buildStructures()` over the reference maps |
| `foliage/<kind>.glb` | `client/foliage.js treeGeometry()` and `leafGeometry()` |
| `props/<kind>.glb` | `client/props.js createProps()` kinds that are not foliage (rocks, fence, haystack, supplies) |
| `source/infantry-authored-rig.glb` | `client/models/infantry-authored-data.js` (Blender rig payload) as a skinned mesh, 15 joints |
| `source/infantry-retopo-v3.glb` | byte-identical copy of `art/model-reference-2026-10-03/infantry-retopo-v3.glb` |
| `textures/**` | copies of `client/textures` the materials reference |
| `manifest.json` | per asset: source module, call, file sha256s at the pin, output sha256, counts, bounds, pivots, tint checks |

Map pieces, foliage and props are the shared prototype each `InstancedMesh` draws; where pieces stand, their size
and their per-instance tint come from the reference's layout code (`structures.js`, `props.js`), which is code to
port, not model data. The HQ sandbag ring is one fixed piece, so its instances are merged into one mesh with each
bag's instance color as its vertex color.

## Space and units

glTF files are in three.js model space as built: +Y up, metres, right-handed, which is glTF's own convention, so no
transform is applied. Unit models face +X with +Z on their right (the reference's convention, not glTF's +Z front).
Unreal's Interchange glTF importer maps glTF (x, y, z) to UE (x, z, y) and scales by 100 (GLTFCore
`ConversionUtilities.h ConvertVec3`, `InterchangeGltfPrivate.h GltfUnitConversionMultiplier = 100`), so +X forward
stays UE +X forward, +Z right becomes UE +Y right and +Y up becomes UE +Z up, in centimetres. Every model is built at
the origin; the root node carries no transform.

## Hierarchy and pivots

Nodes keep the reference object tree and its local transforms. Role nodes are named after the fields `buildModel()`
sets, plus the groups it keeps in local variables: `hull` (a vehicle's fixed part), `turret` (traversing part,
positioned at its ring; also a squad's gun when it traverses), `gun` (a squad's fixed gun holder), `mount_N`
(destroyer gun mounts; `mount_0` is the one `v.turret` points at), `body`, `tail`,
`leg_N` (Kaiju), `door` (gate), `crew`, `man_N` (slot position and figure scale), `man_N_pose` (the posture node
`animate()` leans, squashes and shifts), `man_N_near` and `man_N_far` (the two levels of detail; far is hidden,
`extras.visible: false`), `gun_near` and `gun_far`, `fx_tip` (empty node at `v.fxTip`, the muzzle point
`client/fx.js` uses, in the turret's frame), and for planes `airframe`, `prop_N` (spins about +X), `prop_N_blades`,
`prop_discs`. Propeller groups start at a random turn in the reference (`Math.random()`); they are exported at
rest. `manifest.json` lists every role node's translation, rotation and scale as `pivots`.

Wheels and track links are not separate nodes in the reference: they turn in the vertex shader about per-vertex
pivots, so those pivots are exported as attributes (below).

## Vertex attributes

| glTF attribute | Reference attribute | Meaning |
| --- | --- | --- |
| `POSITION`, `NORMAL` | position, normal | as built |
| `COLOR_0` (float VEC3) | color | linear vertex color of the exported look, clamped to [0, 1] |
| `_COLOR` (VEC3) | color | the raw value, only where a builder wrote a multiplier outside [0, 1] |
| `_MATID` (SCALAR) | matId | `floor(v + 0.25)` = index into `manifest.matLayers` (-1 plain, -2 the material's default); the fraction times 2 = baked grime |
| `_OWNERTINT`, `_VEHICLETINT` (VEC3) | (derived) | per-channel weight of the owner and vehicle paint (Looks and tints) |
| `_MODELUV` (VEC3) | modelUV | infantry atlas u, v; z: 0 no atlas, 1 atlas, 2 atlas with the uniform tint |
| `_UV` (VEC2) | uv | as built; on base buildings (`structures.js MODEL_MAT`) x is a surface id, see the material extras |
| `_WHEELPIVOT` (VEC4) | wheelPivot | wheel centre xyz and radius; a wheel turns about local z through its centre; radius 0 is static |
| `_WHEELSIDE` (SCALAR) | wheelSide | which track side the wheel travels with |
| `_TRACKDATA` (VEC4) | trackData | x: phase along its loop, y: loop index into `mesh.extras.ww2.trackLoops` (-1: not a track link), z: side, w: floor |
| `_CAMO`, `_HINGE` | camo, hinge | plane camouflage code and control-surface hinge lines (`client/models/planes.js paintMaterial`) |
| `TEXCOORD_n` | mirrors | the same data as two-float channels for importers that drop custom attributes (Unreal reads TEXCOORD_0..7) |

The `TEXCOORD_n` mirror follows one fixed order and includes only the channels a primitive has data for:
`uv`, `modelUV.xy`, `matId,modelUV.z`, `ownerTint,vehicleTint` (grey tints as one scalar each, else three channels
of RGB), `color.rg` and `color.b` (raw color, only when clamped), `wheelPivot.xy`, `wheelPivot.zw`,
`wheelSide,trackData.x`, `trackData.yz`, `trackData.w`, `camo.xy`, `camo.zw`, `hinge.xy`, `hinge.z`.
`mesh.extras.ww2.texcoords` lists what each channel of that primitive holds. Pivot coordinates in the mirror are in
glTF space and metres; Unreal does not convert texture coordinates, so a material reading them must swap y and z and
scale by 100 itself.

Track loops are stored in `mesh.extras.ww2.trackLoops` (and the support band in `trackSupport`). The reference keeps
loops in a global table whose row numbers depend on build order (`client/track-motion.js`); the exporter maps each
vertex back to its geometry's own loop index by matching the table's texels, so the files do not depend on build order.

## Soldiers: morph targets

Each soldier figure carries the reference's posture and gait morph targets as deltas, named in `mesh.extras.
targetNames`: `kneel`, `prone`, `retreat`, `walk_0..7`, `aim_0..3`, `crouch_0..3`, `crawl_0..3`, `dig_0..3` (dig only
on squads that dig), then `fallen` (the corpse build). As in the reference, kneel, prone and retreat are stored undone
by the `man_N_pose` node transform that `animate()` applies (`client/unit-models.js poseMorphs`, `POSES`), so a full
posture needs both the target weight and the pose node's lean, squash and shift. `mesh.extras.ww2.muzzles` holds the
weapon tip per frame.

## Looks and tints

A unit's paint is `client/main.js look(slot) = { ...FACTIONS[faction], color: COLORS[slot] ?? 0xdddddd }`: the
faction fixes the uniform and vehicle paint, the player slot fixes the owner color (six colors). The exporter builds
each look-dependent asset under eleven looks in separate processes (the reference caches by color, so each process
holds one look): `base` (faction paint, owner 0xdddddd), `slot0..5` (the six player colors), and four grey probes.
`COLOR_0` is the `base` look as built. With linear colors per channel:

```
color = COLOR_0 + (owner - lin(0xdddddd)) * _OWNERTINT + (vehicle - lin(faction vehicle)) * _VEHICLETINT
```

`_OWNERTINT` comes from two owner probes (0x404040, 0xc0c0c0) and `_VEHICLETINT` from a vehicle probe (0xc0c0c0).
Both are then checked against the real builds: the six player colors and a third vehicle paint (0x808080). The
manifest records the largest error for every asset. A tint the builder does not apply linearly is not written: the
light and heavy tanks, the Cromwell and the Churchill swap the faction's vehicle hex through a `REPAINT` table, and
`hqCamp` clamps its ratio to the US paint. `COLOR_0` stays exact for the faction paint, which is the only vehicle
paint the reference uses. As a check of the method, the derived tints of the naval models equal the owner and vehicle
masks their Blender payload stores (within 6e-8). If an owner tint ever failed the check, the exporter writes one
file per player slot instead (`-slotN.glb`).

## Materials and textures

Each glTF material has plain PBR factors from the three.js material and the full description in
`extras.ww2`: the three.js type and color, the texture file bound to `map` (read from the image the reference loaded),
and the shader patch the reference adds (`model-textures.js` texture-array layers chosen per vertex by `_MATID`,
`surfaces.js` world-planar textures with their metres per repeat and top tint, `structures.js MODEL_MAT` surface ids,
the infantry atlas, wheel and track motion, plane paint). `MeshBasicMaterial` becomes `KHR_materials_unlit`. Only
the skinned infantry rig binds a texture through glTF itself (its UVs are real); everywhere else the reference maps
textures in its shaders, so the files reference them by path. `brick` is drawn on a canvas at runtime
(`surfaces.js brickCanvas`) and is recorded as procedural.

## Browser shims

Listed in `shims.mjs` and `manifest.shims`: an inert `document.createElement` canvas (draws nothing), an inert
`<img>` from `document.createElementNS` and `Image` that keeps its `src` and reports a load on the next tick without
decoding (so the reference's texture callbacks bind `material.map` as in a browser and the exporter can read which
file each material uses). `atob`, `Blob`, `Response` and `DecompressionStream` are native in Node and are not shimmed.

## Not exported

`client/models/naval.js navalModel()` is the procedural source of the naval models; the client renders the
Blender-finished payload (`naval-finished-data.js`) instead, which is what `units/{lcvp,gunboat,destroyer}` contain.
The plane's ground shadow (a canvas-drawn decal) and HQ overlay markers (zone, ring, flag, label) are not models.
Building breaches (`unit-models.js updateBuildingBreach`) cut holes into the building meshes at runtime from the
server's failed sections; that is runtime geometry processing of the exported meshes, not a separate model.
`art/model-reference-2026-10-03/infantry-rig-v3.blend` is not read: its baked output is the payload in
`source/infantry-authored-rig.glb`, so Blender is not needed.

## From the export to the game

The whole chain, in order (the scratch folders are any user-local directory; nothing writes to `reference/ww2-rts`):

```
# 1. export (above): unreal/Saved/ModelExport
node unreal/Tools/models/export-reference-models.mjs --deps <deps> --jobs 5
# 2. import into Content/Models (Meshes, Materials, Textures, ModelCatalog.json, ImportProvenance.json), then verify.
#    Any project that mounts Content/Models works; a content-only host with Content/Models linked to the real folder
#    keeps the game's module out of it. Clear only Meshes, Materials, Textures and the two JSON files first, never
#    Content/Models/Infantry.
UnrealEditor-Cmd <host>.uproject -run=pythonscript -unattended -nullrhi \
  -script="unreal/Tools/models/import-models.py --manifest unreal/Saved/ModelExport/manifest.json --work <work>"
UnrealEditor-Cmd <host>.uproject -run=pythonscript -unattended -nullrhi -script="unreal/Tools/models/import-models.py --verify <report.json>"
# 3. the soldiers (Content/Models/Infantry): see infantry/README.md
bash unreal/Tools/models/infantry/run.sh --unreal
# 4. build the editor target with the Rust core required, run the tests, compare (compare/README.md)
TC_REQUIRE_WW2_FFI=1 Engine/Build/BatchFiles/Linux/Build.sh ThreeCrossroadsEditor Linux Development -project=<...>/unreal/ThreeCrossroads.uproject
UnrealEditor-Cmd unreal/ThreeCrossroads.uproject -nullrhi -ExecCmds="Automation RunTests ThreeCrossroads; Quit"
unreal/Tools/models/compare/run.sh <out> <deps>
```

`ATCUnitRenderer` (`Client/TCUnitRenderer.h`, `TCUnitModels.cpp`) draws the catalog: one instanced component per part
mesh, every unit an instance whose custom data carries the owner color and faction paint, turrets turned by aim minus
heading (with the reference's traverse limits), structures rising while built, planes 20 m up, the owner ring and the
selection ring, soldiers skinned from the pose solver and following `squad-motion.js`, and the reference's near/far
swap at 110 m. Types are adopted per batch (pilot, vehicles, infantry, airnaval, structures; all on by default);
`-TCModels=none|all|pilot,vehicles,...` overrides it for a run, and any type without a model stays a placeholder.

Fixes made while integrating (each one changes the imported content; re-run steps 1 and 2):

- `import-models.py find_parts`: a mesh its group already took was also made a part of its own, so every mesh was
  listed twice (331 duplicate parts of 1,607) and would have been drawn twice. Now 1,134 parts, and for all 261 models
  the visible parts' triangles equal the exporter's `drawnTriangles` (before: 32 of 261).
- `import-models.py` materials: Interchange's glTF factory already converts `COLOR_0` so the static mesh builder's sRGB
  encoding round-trips (`GLTFMeshFactory.cpp`, `SRGBToLinear`), so `VertexColor` reads linear; the model and surface
  materials decoded it a second time, which made every vertex-colored surface (wood, stone, track steel) about four
  times too dark.
- `import-models.py` faded materials: the per-instance fade replaced the material's own opacity, so a renderer writing
  solid instances drew the propeller discs (opacity 0.08) opaque black. The two now multiply.
- `build-worker.mjs`: a material whose color is the owner's in every look (`unit-models.js cloth(f.color)`, the inline
  structures' flag: only `worldbase`) was exported in the no-slot gray; it is now marked `ownerColor` and the importer
  colors it from the instance's owner color.
