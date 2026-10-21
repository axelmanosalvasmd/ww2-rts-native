# Paper war map and lobby backdrop tools (round 14)

`Content/WarMap` is generated from the pinned reference (`reference/ww2-rts`), never drawn by hand:

| Output | From | How |
| --- | --- | --- |
| `T_TCWarMapSymbols` | `client/symbols.js` (every `SYMBOLS` entry and the "unknown" glyph) | Chromium runs the module's own `drawSymbol()`: fills in white (top half), the 18-unit halo stroke it draws under them (bottom half), 128 px cells |
| `Fonts/F_TCWarMapMono` | the face Chromium draws `client/map-view.js`'s `bold ... "Courier New"` with on the machine that ran the tool (fontconfig's match; Liberation Mono Bold here, SIL OFL, licence copied beside it) | `-run=TCWarMapPaint -ImportFont=` (the editor's font import needs Slate, which a commandlet lacks) |
| `WarMapData.json` | the atlas layout and `shared/sim.js` `CELL`, `CFG.pointRadius`, `CFG.levelHeight`, each building's `size` | read by `Client/TCWarMap.cpp` with `FFileHelper` (staged as a file) |
| `WarMapProvenance.json` | sha256 of the inputs, the reference sources and the assets | |

The paper, the desk and the wash are not assets: `Client/TCWarMapPaint.cpp` paints them at run time from this client's
own grid, as `map-view.js` paints its canvases (the map changes during a match).

```
unreal/Tools/warmap/run.sh <scratch work dir> [project]          # assets: node + Chromium, the font commandlet, the import
node unreal/Tools/warmap/make-warmap.mjs oracle --work <dir> --three <reference copy>/node_modules/three/build [--maps a,b]
UnrealEditor-Cmd <project> -run=TCWarMapPaint -Map=reference/ww2-rts/maps/<map>.json -Out=<dir>
node unreal/Tools/warmap/compare-oracle.mjs <oracle>/<map>-paper.png <dir>/<map>-paper.png <sheet.png> <label>
```

`oracle` runs `map-view.js` `createMapView()` unchanged in Chromium (grid = the map's rows, `hAt` = the cell's level x
2.5 m) and saves the paper and desk canvases it painted; the commandlet paints the same inputs with the C++ port; the
compare script reports per-pixel differences and writes reference | port | difference x 4.

`reference-shots.mjs` is the browser half of the side by sides: `lobby` (the pinned client as host picks a map and
saves the lobby over `lobby-view.js`'s backdrop) and `tour` (a spectator at the poses of a tour file, logging
`rig.wide`, the map view's fade and `camera.js`'s clear band). The Unreal half is `-TCWarMapTour=<file>
-TCWarMapShotDir=<dir>` (screenshots with the HUD, where the paper is drawn) and `-TCBackdropShotDir=<dir>`.
