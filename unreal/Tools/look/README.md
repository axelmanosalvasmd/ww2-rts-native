# Light and color: the reference's look in the Unreal client

The reference draws its battlefield with three.js r186 under `client/light.js` and `client/moods.js`. This folder
reproduces that look in Unreal (`Client/TCLighting.h`, the assets under `Content/Models/Materials/Look`, one node
in each model master) and measures it against the reference, picture by picture.

## How each part of the reference's light is reproduced

| three.js (the reference) | Unreal |
| --- | --- |
| `DirectionalLight` sun: mood color, `sunI` (day 3.2), `shadow.intensity` (0.95), aimed by `aimSun(yaw)` | a movable directional light: the same color (linear), `sunI` in lux, `ShadowAmount`, the same direction. Both engines give a white Lambert surface `E / pi`. |
| `HemisphereLight` (top, bottom, `hemiI`) | in the sky light's cube as the radiance `I/pi ((top+bottom)/2 + 3/4 (top-bottom) y)`, whose cosine irradiance is exactly three's `mix(bottom, top, (n.y+1)/2) I` |
| `scene.environment` (PMREM of `outdoorEnvironment()`, `environmentIntensity` 0.38). In r186 it lights MeshLambertMaterial too (diffuse only, `lights_fragment_end`). | in the same cube, times its intensity. The sky light's diffuse was checked against the cube's integral for six normals: within 0.5%. |
| MeshStandardMaterial's environment reflection, DFG LUT, multiple scattering | the model masters (`M_TCModel*`) set Ambient Occlusion to 0, so the sky light leaves their pixels. Their `TCLookIndirect` node adds what three gathers instead: the hemisphere, the PMREM diffuse and reflection (three's own atlas, sampled by a port of `textureCubeUV`), `DFGLUTData`, and the Fdez-Aguera multiple scattering, with dielectric and metal mixed by metalness. Lambert materials (Specular 0) take the diffuse only. |
| ACES filmic at `toneMappingExposure`, sRGB output | `M_TCLookPost` replaces the engine tonemapper. Exposure is locked at 1, with no bloom, vignette, grain, fringe, flare, motion blur, GI or SSR (`../../Config/TCPostProcess.md`). |
| `Fog(haze, 0.6 d / hazeK, 3.8 d / hazeK)` after tone mapping; `background = haze` | the same mix in `M_TCLookPost` from scene depth, with `d` the camera distance (`ATCLighting::SetCameraDistance`); the haze where nothing was drawn |
| `atmosphere.js` weather: sun x, shadow x, haze x and color, eased with `1 - exp(-dt / 2.5)` | `ATCLighting::SetWeather` / `SetShower` / `Tick`, through `TCLook::WeatherGoal` and `Applied` |
| `moodFor` / `mapMood` | `TCLook::MoodFor` / `MapMood` |

## Files

- `look.html`, `look.mjs`: the reference half. These are the model comparison's builders (`../models/compare/compare.mjs`) under the game's light; every view also has a silhouette mask and a ground plate. Two special assets:
  - `type:fac:owner:man`: one soldier, at one man's hit points (`alive = 1`), with close views `man-front`, `man-threequarter` and `man-side`;
  - `calib`: 20 flat MeshBasicMaterial patches of known linear color, and `horizon`, the game's haze over the ground with the sky behind.
  - It also writes three's PMREM atlas (`env-pmrem.f32`, read back from its own render target).
- `shoot.mjs`: serves the page with the pinned `client/` and `shared/` (read only; it refuses a dirty reference) and runs it in headless Chromium with a throwaway profile.
- `make-look-assets.py`: editor Python that makes `Content/Models/Materials/Look/*` and patches the four model masters. It is idempotent and writes `Content/Models/Materials/LookProvenance.json`. **Run it after every `../models/import-models.py`**, which rewrites the masters.
- `TCLookCompare` (in `Client/TCLighting.cpp`): the Unreal half. It renders the same cameras through `ATCUnitRenderer` under `-Light=look` (this look), `before` (the model comparison's stand-in light) or `gamebefore` (`ATCGameMode::SpawnScene`'s light before this slice), with depth silhouettes and ground plates. `-SunScale=` and `-SkyUniform=` are calibration switches.
- `metrics.mjs`: mean color on the unit's pixels per asset and view, its delta (per channel, distance, CIE76 Delta E), the ground's, and the masks' IoU.
- `run.sh`: the steps (`shoot`, `assets`, `unreal`, `metrics`, `pairs`).

## Regenerate

```
D=<three 0.186.1 deps>; O=<out>
unreal/Tools/look/run.sh shoot $O $D men rifle:0:0:man,mg:1:1:man,...,calib,horizon        # and the other sets
HOST=<content host> unreal/Tools/look/run.sh assets $O/men $D        # after import-models.py
unreal/Tools/look/run.sh unreal $O look men pilotinf vehicles airnaval structures
unreal/Tools/look/run.sh metrics $O $O/metrics.json men pilotinf vehicles airnaval structures
unreal/Tools/look/run.sh pairs $O men pilotinf vehicles airnaval structures
```

The `before` and `gamebefore` lights only reproduce the old look with the importer's unpatched masters. With
patched masters, the sky light no longer reaches the models.
