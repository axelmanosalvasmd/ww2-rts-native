// Oracle for unreal/Source/ThreeCrossroads/Client/TCAudio.cpp (ThreeCrossroads.Audio.Environment): runs the pinned
// reference's client/engine-presentation.js environmentalMix under Node on a small test map and prints one JSON line
// per case, which the test's table holds.
//   WW2_MODEL_DEPS=<deps> node --import ./unreal/Tools/models/register.mjs unreal/Tools/audio/env-oracle.mjs reference/ww2-rts
// (<deps>: the pin's three.js, as in unreal/Tools/models/README.md; client/engine-presentation.js imports it)
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const REF = process.argv[2];
if (!REF) { console.error('usage: node env-oracle.mjs <reference checkout>'); process.exit(2); }
const { environmentalMix } = await import(pathToFileURL(join(resolve(REF), 'client/engine-presentation.js')).href);
// a 40 x 30 map: a river along column 20 (W), woods block at x 5..9, z 5..9 (O), the rest grass (.)
const W = 40, H = 30;
const grid = Array.from({ length: H }, (_, z) => Array.from({ length: W }, (_, x) => x === 20 ? 'W' : (x >= 5 && x <= 9 && z >= 5 && z <= 9 ? 'O' : '.')).join(''));
const cases = [
  { camera: { x: 30, z: 30, dist: 60, yaw: 0 }, weather: 'clear', wx: [], known: () => true },
  { camera: { x: 30, z: 30, dist: 140, yaw: 1.1 }, weather: 'rain', wx: [0.6, 0.4, 2.0, 0.8], known: () => true },
  { camera: { x: 30, z: 30, dist: 60, yaw: 0 }, weather: 'snow', wx: [0.9, 0, 0, 0], known: (x, z) => x < 36 }, // the river unknown
  { camera: { x: 16, z: 14, dist: 85, yaw: -0.7 }, weather: 'clear', wx: [], known: () => true },
];
for (const c of cases) console.log(JSON.stringify(environmentalMix({ grid, cell: 2, known: c.known, camera: c.camera, weather: c.weather, wx: c.wx })));
