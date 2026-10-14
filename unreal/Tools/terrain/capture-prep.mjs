// Prepares an Unreal capture of the native painted ground (capture-ground.py): the harness's Native canvas for one
// paint (<case>.<k>.native.ppm, as FTCGroundPaint paints it), its rubble and crater decals (.native-decals.json), a fog
// mask (one byte a cell: 255 seen, 140 explored, 82 never seen) and the cameras.
//   node capture-prep.mjs --oracle <dir> --harness <dir> --case <name> --k <n> --out <dir> [--fog end|synthetic] [--target rubble|x,z]
// --target rubble: aim at the densest rubble decals instead of the craters; x,z: reference metres.
// --fog end: the side's fog the reference server computed after the oracle's World run (shared/sim.js fogFor, decoded
// with its unpackRuns). --fog synthetic: a circle seen and a ring explored round the busiest crater spot (authored
// maps have no match in the oracle, so no server fog); the file names say which.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
const HERE = dirname(fileURLToPath(import.meta.url));
const arg = (n, d) => { const i = process.argv.indexOf(`--${n}`); return i > 0 ? process.argv[i + 1] : d; };
const oracle = arg('oracle'), harness = arg('harness'), name = arg('case'), k = +arg('k', 0), out = resolve(arg('out')), fogMode = arg('fog', 'synthetic');
mkdirSync(out, { recursive: true });
const { unpackRuns } = await import(pathToFileURL(join(HERE, '../../../reference/ww2-rts/shared/sim.js')).href);
const replay = JSON.parse(readFileSync(join(oracle, name + '.replay.json'), 'utf8'));
const W = replay.start.map.w, H = replay.start.map.h;
execFileSync('magick', [join(harness, `${name}.${k}.native.ppm`), join(out, 'canvas.png')]);
// craters the client knows after step k: their centre of mass picks the camera target
const rows = [...replay.start.cells, ...replay.steps.slice(0, k).flat()];
const craters = rows.filter(r => r[1] === '+' || (r[3] ?? 0) >> 5 & 3).map(r => [r[0] % W, Math.floor(r[0] / W)]);
let target = [W / 2, H / 2], best = -1;
for (const [x, y] of craters) { const n = craters.filter(([a, b]) => Math.hypot(a - x, b - y) < 8).length; if (n > best) { best = n; target = [x + 0.5, y + 0.5]; } }
const decalsIn = JSON.parse(readFileSync(join(harness, `${name}.${k}.native-decals.json`), 'utf8'));
const want = arg('target', '');
if (want === 'rubble' && decalsIn.rubble.count) {
  const P = decalsIn.rubble.positions, pts = [];
  for (let s = 0; s < decalsIn.rubble.count; s++) pts.push([P[(s * 25 + 12) * 3] / 2, P[(s * 25 + 12) * 3 + 2] / 2]);
  best = -1;
  for (const [x, y] of pts) { const n = pts.filter(([a, b]) => Math.hypot(a - x, b - y) < 6).length; if (n > best) { best = n; target = [x, y]; } }
} else if (/^[\d.]+,[\d.]+$/.test(want)) target = want.split(',').map(v => +v / 2);
const fog = Buffer.alloc(W * H, 82);
if (fogMode === 'end' && replay.fogEnd) {
  const seen = new Uint8Array(W * H), explored = new Uint8Array(W * H);
  unpackRuns(replay.fogEnd.v, (s, n) => seen.fill(1, s, s + n));
  unpackRuns(replay.fogEnd.e, (s, n) => explored.fill(1, s, s + n));
  for (let c = 0; c < W * H; c++) fog[c] = seen[c] ? 255 : explored[c] ? 140 : 82;
} else {
  for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) { const d = Math.hypot(x + 0.5 - target[0], y + 0.5 - target[1]); fog[y * W + x] = d < 9 ? 255 : d < 16 ? 140 : 82; }
}
// fog as an 8-bit grey PNG through ImageMagick (raw bytes in)
writeFileSync(join(out, 'fog.gray'), fog);
execFileSync('magick', ['-size', `${W}x${H}`, '-depth', '8', `gray:${join(out, 'fog.gray')}`, join(out, 'fog.png')]);
const decals = decalsIn;
// the game's camera (client/camera.js): fov 42 vertical, pitch 0.95 rad; three distances
const cam = (dist, yaw, pitch = 0.95) => ({ x: target[0] * 2, z: target[1] * 2, dist, yaw, pitch, fov: 42 });
writeFileSync(join(out, 'scene.json'), JSON.stringify({ name, k, w: W, h: H, fog: fogMode, canvas: join(out, 'canvas.png'), fogPng: join(out, 'fog.png'),
  decals, target: [target[0] * 2, target[1] * 2], cameras: [cam(32, 0.6), cam(70, 0.6), cam(150, 0.6, 1.2)] }));
console.log(`${name}.${k}: ${W}x${H}, target ${target.map(v => (v * 2).toFixed(1))} m, ${decals.rubble.count} rubble and ${decals.craters?.count ?? 0} crater decals, fog ${fogMode}`);
