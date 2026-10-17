// The oracle for the railways and atmosphere port (World/TCRailways.h TCRailCore, World/TCAtmosphere.h TCAtmosCore):
// runs the pinned reference's own code under Node and writes what it builds with the inputs it was given, so the
// native port can be checked without a browser.
//
//   WW2_MODEL_DEPS=<dir with node_modules/three> node --import ./unreal/Tools/models/register.mjs \
//     unreal/Tools/atmosphere/oracle.mjs [--reference reference/ww2-rts] [--out unreal/Tools/atmosphere/fixtures/atmos-oracle.json]
//
// client/railways.js is imported unmodified (createRailways). client/atmosphere.js keeps its helpers private, so its
// text is loaded with two edits only: the light.js import becomes a no-op setMood (the light is ATCLighting's and is not
// checked here), and an export line names the private helpers (noiseTexture, groundMask, mistSites, bankSites,
// weatherPoints, FALLS). Every function body runs as pinned. Browser-only APIs: the models tooling's inert shims
// (document, Image), plus location (moods.js moodFor reads location.search) and no matchMedia (reduceMotion false).
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join, resolve, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { installShims, settle } from '../models/shims.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const opt = (name, def) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : def; };
const REF = resolve(opt('--reference', join(HERE, '../../../reference/ww2-rts')));
const OUT = resolve(opt('--out', join(HERE, 'fixtures/atmos-oracle.json')));
const SCRATCH = resolve(opt('--scratch', process.env.TC_SCRATCH ?? join(HERE, '../../Saved/atmos-oracle'))); // unreal/Saved is ignored by git

installShims();
globalThis.location = { search: '' };
const imp = (p) => import(pathToFileURL(join(REF, p)).href);
const THREE = await import('three');
const { mapForClient } = await imp('shared/world-layers.js');
const { RAIL_DIRS, CELL } = await imp('shared/sim.js');
const { createRailways } = await imp('client/railways.js');
const { setWind, wind } = await imp('client/wind.js');

// atmosphere.js with its private helpers exported (see the header); the edited copy lives in scratch, never in reference/
const src = readFileSync(join(REF, 'client/atmosphere.js'), 'utf8');
const lightImport = "import { setMood } from './light.js';";
if (!src.includes(lightImport)) throw new Error('atmosphere.js: the light.js import moved');
const abs = (p) => pathToFileURL(join(REF, 'client', p)).href;
let edited = src.replace(lightImport, 'const setMood = () => {};')
  .replace("from './gfx.js'", `from '${abs('gfx.js')}'`).replace("from './moods.js'", `from '${abs('moods.js')}'`)
  .replace("from '../shared/weather.js'", `from '${pathToFileURL(join(REF, 'shared/weather.js')).href}'`).replace("from './wind.js'", `from '${abs('wind.js')}'`);
edited += '\nexport { noiseTexture, groundMask, mistSites, bankSites, weatherPoints, FALLS };\n';
mkdirSync(SCRATCH, { recursive: true });
const editedPath = join(SCRATCH, 'atmosphere-oracle-module.mjs');
writeFileSync(editedPath, edited);
const atmos = await import(pathToFileURL(editedPath).href);
await settle();

const r6 = (v) => Math.round(v * 1e6) / 1e6;
const arr = (a) => Array.from(a, r6);
const sha = (p) => createHash('sha256').update(readFileSync(join(REF, p))).digest('hex');
const sources = Object.fromEntries(['client/railways.js', 'client/atmosphere.js', 'client/wind.js', 'client/moods.js', 'shared/weather.js', 'shared/sim.js'].map(p => [p, sha(p)]));
// the analytic ground both sides use (metres)
const WAVE = (x, z) => 0.6 * Math.sin(x * 0.13) + 0.4 * Math.cos(z * 0.09) + 0.002 * x - 0.35;
const load = (file) => mapForClient(JSON.parse(readFileSync(join(REF, 'maps', file), 'utf8')));

const fixture = { format: 'tc-atmos-oracle-v1', reference: 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d', sources, wave: '0.6 sin(0.13 x) + 0.4 cos(0.09 z) + 0.002 x - 0.35' };

// ---------- railways ----------

// two lines with diagonals and a branch, as shared/sim.js setupRails turns cell paths into direction bits
function railCase() {
  const w = 48, h = 40, path = (pts) => pts.map(([x, y]) => y * w + x);
  const line = (x0, y0, steps) => { const out = [[x0, y0]]; let x = x0, y = y0; for (const [dx, dy, n] of steps) for (let i = 0; i < n; i++) { x += dx; y += dy; out.push([x, y]); } return out; };
  const lines = [path(line(3, 5, [[1, 0, 10], [1, 1, 6], [0, 1, 8], [-1, 1, 3]])), path(line(19, 11, [[1, -1, 4], [1, 0, 12]]))];
  const bits = new Map();
  for (const cells of lines) for (let i = 1; i < cells.length; i++) {
    const a = cells[i - 1], b = cells[i], dx = b % w - a % w, dz = Math.floor(b / w) - Math.floor(a / w);
    const out = RAIL_DIRS.findIndex(([x, z]) => x === dx && z === dz), back = (out + 4) % 8;
    bits.set(a, (bits.get(a) ?? 0) | 1 << out); bits.set(b, (bits.get(b) ?? 0) | 1 << back);
  }
  const stations = new Map([[lines[0][2], 0], [lines[0][lines[0].length - 1], 1], [lines[1][lines[1].length - 1], 2]]);
  const all = [...bits.keys()], half = Math.floor(all.length / 2);
  const row = (c, extra = {}) => [c, '.', 0, 0, { ground: '.', ...(bits.has(c) ? { rail: bits.get(c) } : {}), ...(stations.has(c) ? { station: stations.get(c) } : {}), ...extra }];
  const batch1 = all.slice(0, half).map(c => row(c)).concat([[lines[0][2] + 1, '.', 0, 0, { ground: '.' }], [5, '.', 0, 0]]);
  const batch2 = all.slice(half).map(c => row(c));
  // the first batch's cells again, one with a changed mask (a junction learned later)
  const resend = [row(all[1]), [all[3], '.', 0, 0, { rail: (bits.get(all[3]) | 1 << 6) }]];
  const steps = [
    { cells: batch1, centres: [[0, 40.5, 30.2]], dt: 0.016 },
    { cells: [], centres: [[0, 40.5, 30.2]], dt: 0.2 },
    { cells: batch2, centres: [], dt: 0.2 },
    { cells: [], centres: [], dt: 0.2 },
    { cells: [], centres: [[1, 2.0, 70.0], [0, 40.5, 30.2]], dt: 0.2 },
    { cells: resend, centres: [[1, 2.0, 70.0], [2, 90.0, 4.0]], dt: 0.6 },
    { cells: [], centres: [[2, 90.0, 4.0]], dt: 0.6 },
  ];
  const group = new THREE.Group();
  const lastCentres = { m: new Map() };
  const rw = createRailways({ parent: group, hAt: WAVE, w, center: (id) => { const c = lastCentres.m.get(id); return c ? { x: c[0], z: c[1] } : null; } });
  const out = [];
  for (const s of steps) {
    lastCentres.m = new Map(s.centres.map(([id, x, z]) => [id, [x, z]]));
    rw.cells(s.cells);
    rw.frame(s.dt);
    const g = rw.group;
    g.updateMatrixWorld(true);
    const ims = g.children.filter(o => o.isInstancedMesh);
    const mats = ims.map(im => arr(im.instanceMatrix.array.subarray(0, im.count * 16)));
    const stationsOut = g.children.filter(o => o.isGroup).map(st => ({ region: st.userData.region, parts: st.children.map(p => arr(p.matrixWorld.elements)) }));
    out.push({ ballast: mats[0] ?? [], ties: mats[1] ?? [], rails: mats[2] ?? [], stations: stationsOut, has: [...rw.track()].length });
  }
  return { w, h, steps, out };
}
fixture.rail = railCase();

// ---------- atmosphere: the private helpers ----------

const noise = atmos.noiseTexture();
fixture.noise = Array.from({ length: 128 * 128 }, (_, i) => noise.image.data[i * 4]);
fixture.falls = {};
for (const kind of ['rain', 'snow', 'dust']) {
  const pts = atmos.weatherPoints(kind);
  fixture.falls[kind] = { position: arr(pts.geometry.attributes.position.array), rand: arr(pts.geometry.attributes.aRand.array) };
}
fixture.wind = [[0, 0], [1.2, 0.7], [-2.5, 1], [3.1, 0.15]].map(([a, v]) => { setWind(a, v); return [a, v, r6(wind.x), r6(wind.z)]; });

const MAPS = [['default.json', 'default'], ['pegasus-bridge.json', 'pegasus-bridge'], ['the-polder.json', 'the-polder'], ['kasserine-pass.json', 'kasserine-pass'],
  ['ardennes-crossing.json', 'ardennes-crossing'], ['crossroads-village.json', 'crossroads-village'], ['hill-112.json', 'hill-112']];
fixture.maps = [];
for (const [file, key] of MAPS) {
  const map = load(file);
  const mask = atmos.groundMask(map);
  const mist = atmos.mistSites(map, CELL, WAVE, 36).map(s => [s.x, s.z, s.y, s.yaw, s.sx, s.sz, s.ph].map(r6));
  const banks = atmos.bankSites(map.w * CELL, map.h * CELL, WAVE, 64).map(s => [s.x, s.z, s.y, s.yaw, s.sx, s.sz, s.ph].map(r6));
  fixture.maps.push({ file, key, w: map.w, h: map.h, rows: map.rows, mask: Array.from(mask.image.data), mist, banks });
}

// ---------- atmosphere: a running instance (start, then update) ----------

function running(file, key, weather, windRow) {
  const map = load(file), MW = map.w * CELL, MH = map.h * CELL;
  const scene = new THREE.Scene();
  const renderer = { getDrawingBufferSize: (v) => v.set(1280, 720), getPixelRatio: () => 1 };
  const camera = new THREE.PerspectiveCamera(42, 16 / 9, 1, 900);
  const cam = { x: MW * 0.4, z: MH * 0.55, y: WAVE(MW * 0.4, MH * 0.55) };
  wind.x = 0.55; wind.z = 0.25; // the wind before any snapshot (client/wind.js)
  const at = createAtmosphereFrom(scene, renderer, camera, cam);
  const ground = new THREE.Mesh(new THREE.PlaneGeometry(MW, MH));
  const apron = { overlayGeometry: new THREE.BufferGeometry(), mesh: { geometry: new THREE.BufferGeometry() } };
  at.start({ map, key, ground, hAt: WAVE, weather: [weather], apron });
  const root = scene.children.find(o => o.name === 'atmosphere');
  const [cloudBoard, , cover, , mist, banks, birds] = root.children;
  const frames = [];
  for (let i = 0; i < 12; i++) {
    if (i === 3 && windRow) setWind(windRow[0], windRow[1]);
    if (i === 6) at.setRain(0.8, 0.6);
    at.update(0.25);
    const fall = root.children[7];
    const f = { t: r6((i + 1) * 0.25), drift: arr(cloudBoard.material.uniforms.drift.value.toArray()), cloudVisible: cloudBoard.visible,
      birds: birds.visible ? arr(birds.instanceMatrix.array) : [], mist: mist.visible ? arr(mist.instanceMatrix.array.subarray(0, mist.count * 16)) : [],
      banks: banks.visible ? arr(banks.instanceMatrix.array.subarray(0, banks.count * 16)) : [], mistCount: mist.count, bankCount: banks.count,
      bankOpacity: r6(banks.material.uniforms.uOpacity.value), banksVisible: banks.visible, mistVisible: mist.visible,
      fall: fall ? { kind: fall.userData.kind, vel: arr(fall.material.uniforms.uVel.value.toArray()), alpha: r6(fall.material.uniforms.uAlpha.value),
        boxMin: arr(fall.material.uniforms.uBoxMin.value.toArray()), scale: r6(fall.material.uniforms.uScale.value) } : null,
      cover: { visible: cover.visible, wet: r6(cover.material.uniforms.uWet.value), sheen: r6(cover.material.uniforms.uSheen.value), mud: r6(cover.material.uniforms.uMud.value), snow: r6(cover.material.uniforms.uCover.value) },
      strength: r6(cloudBoard.material.uniforms.strength.value), fade: arr(cloudBoard.material.uniforms.fade.value.toArray()) };
    frames.push(f);
  }
  return { file, key, weather, wind: windRow, mood: at.mood, cam: [cam.x, cam.z, r6(cam.y)], frames };
}
function createAtmosphereFrom(scene, renderer, camera, cam) { return atmos.createAtmosphere({ scene, renderer, camera, cam }); }
fixture.running = [running('pegasus-bridge.json', 'pegasus-bridge', 'clear', [2.0, 0.4]), running('default.json', 'default', 'rain', [-1.0, 0.9]),
  running('ardennes-crossing.json', 'ardennes-crossing', 'snow', null), running('kasserine-pass.json', 'kasserine-pass', 'clear', [0.3, 1.0]),
  running('the-polder.json', 'the-polder', 'fog', null)];

mkdirSync(dirname(OUT), { recursive: true });
writeFileSync(OUT, JSON.stringify(fixture));
console.log(`wrote ${OUT}: rail steps ${fixture.rail.steps.length}, maps ${fixture.maps.length}, running ${fixture.running.length}, ${readFileSync(OUT).length} bytes`);
