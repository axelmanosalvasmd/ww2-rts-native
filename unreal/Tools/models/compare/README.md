# Side-by-side model pictures

Renders the same units in the pinned reference's own three.js code and in the Unreal client's unit renderer, from the
same cameras, so each pair shows what the conversion kept and what it changed.

```
unreal/Tools/models/compare/run.sh <out dir> <three deps dir> [type:faction:ownerSlot,...]
```

- `compare.html`, `compare.mjs`: the reference half. Builds each unit with `client/unit-models.js` `buildModel` and
  `animate` (planes and the airfield through `client/aircraft.js`, as `main.js makeUnit` does), lights it with
  `client/light.js` (the default mood), stands it on `client/viewer.js`'s grass and 2 m grid, and shoots four views:
  `front`, `side` (the unit's left), `threequarter` (from above) and `gameplay` (`client/camera.js` defaults: fov 42,
  pitch 0.95, distance 40). Writes `<asset>-<view>-reference.png` and `cameras.json` (cameras, light, ground).
- `shoot-reference.mjs`: serves the page with `reference/ww2-rts/client` and `shared` (read only, refuses a dirty tree)
  and the pin's three.js, and runs it in headless Chromium with a throwaway profile under the output folder.
- The Unreal half is the `TCModelCompare` commandlet (`Client/TCModelCompare.h`): it spawns `ATCUnitRenderer`, feeds it a
  one-unit snapshot and captures the same cameras: `<asset>-<view>-unreal.png`. With `-GameLight` it uses the game's
  light instead (`ATCGameMode::SpawnScene`: sun, sky atmosphere, real-time sky light, auto exposure):
  `<asset>-<view>-unreal-gamelight.png`.
- `run.sh` runs both and puts each pair, reference left and Unreal right, in `<out>/pairs`.

The Unreal half's light is a stand-in for three's, not a calibration: a directional light and a sky light from the
engine's daylight cubemap, with values chosen so the grass reads about as in the reference (`cameras.json` light:
`sunLux`, `skyIntensity`, `exposureBias`). Tone mapping differs (three ACES filmic, Unreal's filmic), the reference's rim
light (`uModelRim`) is drawn by the model material but reads weaker, the sky above the horizon is not drawn (black), and
in this fixed light metal has no environment to reflect (the US P-51 reads black; its `-gamelight` shot shows the game's
sky reflected). In `-GameLight` shots the real-time sky light does not fill in inside a commandlet, so faces away from
the sun are black there.
