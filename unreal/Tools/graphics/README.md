# Graphics Low: the oracle

`client/gfx.js` at the pin (e4e579a) has two levels; what Low changes is spread over the builders that read `gfx.low`
(see `Source/ThreeCrossroads/Client/TCGraphics.h`). This folder checks the parts that decide what is drawn: which
scenery props and grass survive on Low, which pieces structures.js builds on Low, which meshes cast shadows on each
level, and ground.js's texels per cell, each against the unmodified reference under Node.

| File | What it does |
| --- | --- |
| `gfx-low-oracle.mjs` | Runs the pinned `client/props.js` (`candidates`, `createProps`, `setNodes`), `client/structures.js` (`buildStructures`) and `client/gfx.js` (`gfx.set`, which fires `gfx.onChange`) under Node on six authored maps (seelow-heights, crossroads-village, hurtgen-forest, ardennes-crossing, bocage, pegasus-bridge). Per map: each candidate's drawn kind on High, after `gfx.set('low')` with the props already built (the switch in the middle of a match, through props.js's own subscription), built on Low from the start, and back on High; per instanced mesh its count and `castShadow` on each level (props and pieces); ground.js `createGround`'s `const P = ...` line read from the pinned file and evaluated for the map on both levels |
| `fixtures/gfx-low-oracle.json.gz` | Its output (6 maps, 6,241 candidates; High shows 6,208, Low 2,383) |

The native checks are the automation tests `ThreeCrossroads.World.GraphicsLowOracle` (every candidate's kind on both
levels, every mesh row's count and shadow flag, the texels) and `ThreeCrossroads.World.GraphicsSwitch` (a switch of level
in the middle of a match through `FTCGraphics`'s change event on Seelow Heights: props, pieces, shadows, the relief's
low variant against a Low build, the water's drift, and back), both in `World/TCScatter.cpp`.

```
R=<the repository clone>
D=<a folder with three from the pin's package-lock>   # see unreal/Tools/models/README.md
cd $R && WW2_MODEL_DEPS=$D node --import ./unreal/Tools/models/register.mjs unreal/Tools/graphics/gfx-low-oracle.mjs
gzip -9 -n -f unreal/Tools/graphics/fixtures/gfx-low-oracle.json
```
