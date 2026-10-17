# Relief: tools

The native ground is the reference's relief (`client/relief.js`, `client/water-levels.js`, `client/relief-material.js` at
pin e4e579a), ported line for line:

| Reference | Native |
| --- | --- |
| `relief.js` createRelief (cells, quads, controls, tangents, Hermite patches, water carve, roads, trenches, crater lips, cliff walls, rock models, fillers, fan budget, hAt, mesh.raycast, update) and `water-levels.js` | `Source/ThreeCrossroads/World/TCRelief.h/.cpp` (engine-free) |
| `client/main.js` createRelief / applyCells -> relief.update, the fog | `Source/ThreeCrossroads/World/TCTerrain.h/.cpp` (`ATCTerrain`, 32-cell chunks) |
| `relief-material.js` | `make-relief-material.py` -> `Content/Relief/M_TCRelief` |

Recipient knowledge (not in relief.js, which draws all of start.map): unknown World cells are never drawn and never
shape known ground (TCRelief.h). Game layout differences from the reference, same surface: one quad per flat cell
(the colour and fog are per vertex until the painted ground lands), and HeightAt/RayToGround read the drawn triangles
of cells a crater lip shoved (relief.js reads them unshoved).

## Files

| File | What it does |
| --- | --- |
| `register.mjs` | Node loader hook: bare `three` from `WW2_MODEL_DEPS` (the models tooling's three r186 from the pin's lock); appends `export { heightAt as __oracleHeightAt };` to the text of `shared/sim.js` as it loads (heightAt is private), the file on disk untouched |
| `relief-oracle.mjs` | The oracle: the pinned `createRelief`, `ground.js` cellAttrs (isRoad), `sim.js` levelAt/heightAt, unmodified, on all 33 maps, two incremental cases (`relief.update`), Low graphics and a World Conquest sample (start, then a scouting step). Writes every geometry attribute, hAt at every cell centre plus 4,000 to 40,000 points, the water line at every centre and 600 to 3,000 camera picks per case, with the inputs, and `fixture.json` |
| `fixtures/relief-oracle.json.gz` | The engine test's share of the oracle: inputs, about 1,500 hAt samples and 200 picks per case, 40 cases |
| `harness/` | `run.sh <oracle dir> <work dir>`: builds `relief-harness.cpp` (C++17, no engine, `-ffp-contract=off`) on TCRelief.cpp, runs every case in the Reference layout (compared attribute by attribute) and the game layout (contract checks, HeightAt and rays against a brute-force scan of the drawn triangles, incremental against fresh), `compare.mjs` prints and writes `summary.json` |
| `make-relief-material.py` | Generates `/Game/Relief/M_TCRelief` and `ReliefMaterial.json` (provenance) |
| `compare/` | `shoot.mjs` + `relief.html`/`relief.mjs`: the reference half of a side by side in headless Chromium (light.js default mood, camera.js cameras); the native half is `-run=TCReliefShot` |

## Regenerate and check

```
R=<this repository>; UE=~/Applications/UnrealEngine-5.8.3
D=<scratch>/three-deps     # three r186 from the pin's package-lock
WW2_MODEL_DEPS=$D node --import $R/unreal/Tools/relief/register.mjs $R/unreal/Tools/relief/relief-oracle.mjs --out <scratch>/oracle
gzip -9 -n -c <scratch>/oracle/fixture.json > $R/unreal/Tools/relief/fixtures/relief-oracle.json.gz
$R/unreal/Tools/relief/harness/run.sh <scratch>/oracle <scratch>/harness
$UE/Engine/Binaries/Linux/UnrealEditor-Cmd <project> -run=pythonscript -unattended -nop4 -nosplash -nullrhi -script="$R/unreal/Tools/relief/make-relief-material.py"
# engine: ThreeCrossroads.Terrain.ReliefOracle, ThreeCrossroads.Terrain.Picking
$UE/Engine/Binaries/Linux/UnrealEditor-Cmd <project> -run=TCReliefShot -Map=$R/reference/ww2-rts/maps/hot-gates.json -Out=<dir> \
  -Views="205,121,45,0.6" -AllowCommandletRendering -RenderOffscreen -unattended -nosplash -nop4 -NoSound
node $R/unreal/Tools/relief/compare/shoot.mjs --three $D --out <dir> --assets "map=hot-gates.json&views=205,121,45,0.6"
```
