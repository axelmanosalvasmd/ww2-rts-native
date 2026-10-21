# Combat effect assets (Content/Effects)

The Unreal client draws combat effects with the reference game's own effect pictures and a port of its effect
shaders. Nothing here is new art: every texel comes from the pinned reference (`reference/ww2-rts` at e4e579a), read
only.

| Asset | From | How |
| --- | --- | --- |
| `Textures/T_FxAtlas` | `client/textures/fx-atlas.webp` (8 x 8 cells of 256 px) | premultiplied by its alpha, as `client/fx.js` uploads it (`premultiplyAlpha`); linear (sRGB off); BC7 |
| `Textures/T_FxCrater` | `client/textures/fx-crater.webp` | unchanged, sRGB, BC7 |
| `Textures/T_FxRubble` | `client/textures/fx-rubble-materials-v1.webp` | unchanged, sRGB, BC7 |
| `Materials/M_TCFxSprite` | `client/fx.js` particle material (`FRAG`) | unlit, AlphaComposite (fx.js's One / OneMinusSrcAlpha blend), one Custom node (`SPRITE_HLSL` in `import-effects.py`) |
| `Materials/M_TCFxCrater` | `client/fx.js` crater material (`dmat`) | unlit, Modulate (fx.js's 2 x src x dst blend of 0.5 c is c x the ground) |
| `Materials/M_TCFxRubble` | `client/rubble-decals.js` (MeshLambertMaterial) | lit, rough, no specular, masked at three's alphaTest 0.08 |

`EffectsProvenance.json` (next to the assets) records the pin, each source's and PNG's sha256, the tools' sha256 and
each material's HLSL sha256.

The vertex half of fx.js's particle shader (`VERT`: billboards turned to the view, streaks along the motion, sprites
standing on their point, the pull toward the camera, the soft edge where a sprite meets the ground) and its back to
front sort are done per view on the CPU by `UTCFxSpriteComponent` (`Client/TCEffects.cpp`, `TCFxView::Quad`), which
hands the pixel half its inputs in eight texture coordinates. The emitters, presets and particle simulation are
`FTCCombatEffects` (a line by line port of `createEffects`).

## Rebuild

```
unreal/Tools/effects/run.sh [host-dir] [log-dir]
```

1. `convert-fx-textures.mjs` checks the reference is clean at the pin, reads the three WebP files as raw 8-bit RGBA
   through ImageMagick (as the reference's `tools/build-fx-atlas.mjs` reads its sources), premultiplies the atlas with
   integer arithmetic, writes PNGs without time chunks to `unreal/Saved/EffectsExport` with `manifest.json`, and reads
   each PNG back to check its pixels. `--check` exits 1 when the files are not what the pin gives (two runs give the same
   bytes).
2. `import-effects.py --export` runs in a content-only host project (made in scratch on first use, its
   `Content/Effects` linked to this repo's) under `UnrealEditor-Cmd -run=pythonscript`, imports the PNGs and builds the
   three materials.
3. `import-effects.py --verify` loads every asset and checks its type, colour space, size, blend mode and shading model.

Needs node, ImageMagick 7 (`magick`) and the engine (`UE=`, default `~/Applications/UnrealEngine-5.8.3`).

## Side by side

```
unreal/Tools/effects/compare/run.sh <out dir> <three deps dir> [project.uproject]
```

`compare/scenes.json` scripts five scenes (a smoke screen over units, a 105 mm shell, a firefight, a tank wreck, a
burning hedge row). `fx-compare.mjs` plays them through the pinned `client/fx.js` itself in headless Chromium (units
from `client/unit-models.js`, light from `client/light.js`, `Math.random` seeded before fx.js loads), and
`-run=TCEffectsShot` plays the same file through `FTCMatchState` and `ATCEffects` in an offscreen Unreal world, from the
same camera (`client/camera.js` defaults). Pairs land in `<out>/pairs`; `reference-scenes.json` and
`effects-shots.json` list what was alive at each picture.
