# Fog of war and haze checks (round 14, edge scope)

Checks of `Source/ThreeCrossroads/World/TCFogFrontier.h` (a port of the pinned reference's `client/fog.js`) and of
`Client/TCLighting` (the haze per mood and weather) against Node runs of the unmodified reference code.

| File | What |
| --- | --- |
| `fog-oracle.mjs` | imports the reference's own `client/fog.js` and `shared/sim.js` (packRuns, unpackRuns, UNITS) and plays six scenarios: units walking with vision circles, the mask one step behind them as the server's 5 Hz vision pass, some delivered units that give no vision, an authored map's player, a match decided (lift), a spectator without a mask, World's frontier, World ignoring lift, the editor (off). It records the fog texture's alpha after each frame (0.016, 0.05, 0.1 s, then settled), `fog.at`'s looks and the minimap alphas |
| `to-input.mjs`, `harness.cpp` | the engine-free core (`TC_FOG_FRONTIER_CORE_ONLY`) against every recorded value, and the building footprints against `UNITS` |
| `to-inl.mjs` | writes `World/TCFogFrontier.Oracle.inl` (three scenarios, run-length encoded) for `ThreeCrossroads.World.FogFrontier` |
| `haze-oracle.mjs`, `tone.mjs` | the reference's `client/moods.js` imported unmodified (`MOODS`, `WEATHER_LOOK`), `atmosphere.js` `blend()`/`applyMood()` and `light.js` `followView()` restated with three r186's own `Color`: hazeK, the haze hex, the fog's near and far, the haze on screen on High (tone-mapped by the composer's `OutputPass`) and on Low (its bytes) for all 6 moods x 5 weathers |
| `haze-test-gen.mjs` | the rows of `ThreeCrossroads.Look.Haze` (`Client/TCLighting.cpp`) |
| `run.sh` | all of the above: `unreal/Tools/fog/run.sh <reference copy with node_modules>` |

Results at round 14: harness 755,568 values, 0 off (two negative controls fail as expected: a 3-cell frontier, a
footprint not centred); `ThreeCrossroads.World.FogFrontier` 289,140 values, 0 off; `ThreeCrossroads.Look.Haze` 30 of
30 rows.
