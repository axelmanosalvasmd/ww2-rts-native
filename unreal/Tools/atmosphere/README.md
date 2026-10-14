# Railways and atmosphere: tools

The native client draws World Conquest's explored railway and the atmosphere over every match as the pinned reference
does (e4e579a):

| Reference | Native |
| --- | --- |
| `client/railways.js` (cells, frame, rebuild, buildStations, station), `shared/sim.js` RAIL_DIRS and setupRails | `Source/ThreeCrossroads/World/TCRailways.h` (`TCRailCore`, engine-free) and `FTCRailways` (`TCRailways.cpp`) |
| `client/atmosphere.js` (noise, cloud shade, wisp and mist, falls, ground mask and cover, fog banks, birds, start, setWeather, setRain, update), `client/wind.js`, `shared/weather.js` roadMask | `Source/ThreeCrossroads/World/TCAtmosphere.h` (`TCAtmosCore`, engine-free) and `FTCAtmosphere` (`TCAtmosphere.cpp`) |
| `client/moods.js` MOODS and WEATHER_LOOK | `TCLook` in `Client/TCLighting.h` (the light; slice 12), read here for the mood and the weather's look |

Both cores are engine-free (`TC_RAIL_CORE_ONLY`, `TC_ATMOS_CORE_ONLY`), so they are checked against the reference outside
Unreal too.

## Files

| File | What it does |
| --- | --- |
| `oracle.mjs` | Runs the pinned `railways.js` (unmodified) and `atmosphere.js` (its text with the `light.js` import made a no-op and its private helpers exported; every function body as pinned) under Node with the models tooling's inert shims, and writes `fixtures/atmos-oracle.json` (gzip it): a 7-step railway case (two lines, a branch, three stations, cells arriving in batches, centres learned late, a mask changed later), the noise texture, the three falls' points, `setWind`, ground masks and mist and fog bank sites on 7 maps, and 5 running atmospheres (start, then 12 updates of 0.25 s with a wind change and a shower: bird, mist and bank matrices, the cloud drift, the fall's velocity) |
| `harness/` | `run.sh <scratch>`: builds `atmos-harness.cpp` (C++17, no engine) on the two cores, feeds it the fixture's inputs (`to-input.mjs`) and compares everything with `compare.mjs` (2e-4 absolute plus 1e-5 relative; three keeps Float32) |
| `make-atmosphere-assets.py` | Generates `/Game/Atmosphere` (`Content/Atmosphere`): `M_TCAtmosGround` (cover then cloud shade), `M_TCAtmosMist` (mist and fog banks), `M_TCAtmosFall` (rain, snow, dust by World Position Offset), `M_TCAtmosBird`, and `AtmosphereMaterials.json` (HLSL and uasset sha256). `--verify <report.json>` checks the assets against it |
| `compare/watch-reference.mjs` | The reference half of a live side by side: the pinned browser client from the isolated reference server joins the same room mid-match as a spectator in headless Chromium (Graphics kept on High), and saves frames with its camera where the native shot looked |

## How the native client draws it

- **Railways** (`FTCRailways`): instanced unit boxes (three's `BoxGeometry` built at run time, axes swapped) with the
  world masters' `M_TCWorldLambert` (MeshLambertMaterial colour, fog of war by one texel per cell); the water tower is
  three's 10-sided `CylinderGeometry`. The track casts no shadow, the station's parts do, as in `railways.js`.
- **Cloud shade and wet ground** (`M_TCAtmosGround`): one transparent layer on a copy of the ground's own mesh sections
  5 cm above them (the reference reuses the ground geometry for `cloudBoard` and `cover`). Sections are copied again only
  when their shape changes, checked at most twice a second. An apron (ground past the map edge) is added with
  `AddGroundActor` once it exists.
- **Mist and fog banks**: the instanced planes as one procedural mesh, placed on the CPU every frame as `update()` places
  the instances. **Birds**: the 21-vertex mesh flapped on the CPU (`TCAtmosCore::FlapLift`, the vertex shader's formula).
- **Rain, snow, dust**: each point a camera-facing quad; `M_TCAtmosFall`'s World Position Offset is the reference vertex
  shader (wrap in the box that follows the camera target, sway, the pixel size clamp), its pixel shader the streak or dot.
- **Haze**: three's scene fog is not repeated in these materials; `M_TCLookPost` mixes the haze over everything after
  tone mapping. **Fog of war**: the layers' colour is multiplied by the cell's texel (1, 0.55, 0.32), as the reference's
  fog overlay darkens what lies under it.

## Regenerate and check

```
R=<this repository>; UE=~/Applications/UnrealEngine-5.8.3
D=<scratch>/three-deps   # three from the pin's package-lock (models README)
cd $R
WW2_MODEL_DEPS=$D node --import ./unreal/Tools/models/register.mjs unreal/Tools/atmosphere/oracle.mjs --scratch <scratch dir>
gzip -9 -n -f unreal/Tools/atmosphere/fixtures/atmos-oracle.json
unreal/Tools/atmosphere/harness/run.sh <scratch dir>
$UE/Engine/Binaries/Linux/UnrealEditor-Cmd <project> -run=pythonscript -EnablePlugins=PythonScriptPlugin -unattended -nop4 -nosplash -nullrhi \
  -script="$R/unreal/Tools/atmosphere/make-atmosphere-assets.py"
# in-engine: the fixture again through the engine build, and both views in a scratch world
$UE/Engine/Binaries/Linux/UnrealEditor-Cmd <project> -nullrhi -unattended -nosplash -nop4 -NoSound \
  -ExecCmds="Automation RunTests ThreeCrossroads.World.RailwaysOracle+ThreeCrossroads.World.AtmosphereOracle; Quit"
```

Live shots: `-TCAtmosShotAfter=S[,S2] -TCAtmosShotFile=<path.png> [-TCAtmosShotFocus=x,z|spawn|station|track]
[-TCAtmosShotDist=m]` saves `<path>-<S>s-nohud.png` and `-hud.png` and logs the atmosphere's state; the lead patch adds
`-TCHostWeather=<clear|fog|rain|mud|snow|random|map>` for the host's lobby.

## Cook

`FTCAtmosphere` loads `/Game/Atmosphere/Materials/*` by path at a match start: a packaged build needs
`+DirectoriesToAlwaysCook=(Path="/Game/Atmosphere")` in `Config/DefaultGame.ini` (in the lead patch).

## Not ported

- **Graphics Low and reduced motion**: the native client has neither; it always draws High and moves.
- **Three's scene fog inside these layers** (`FOG_FADE`): the post process hazes the composited picture instead, which
  differs from the reference by a small second-order term on the cloud shade's edge in the far haze.
- **The lobby backdrop** (`client/lobby-view.js`, CLI-18): not done in this slice.
