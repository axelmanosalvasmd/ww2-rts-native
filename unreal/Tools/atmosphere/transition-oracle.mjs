// Source-only weather timing oracle. Run via the existing models/register.mjs loader.
// Function bodies in the read-only source stay unchanged. Imports are redirected in
// scratch, light output is a no-op, and one getter exposes the closure's fade state.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
const args = process.argv.slice(2);
const opt = (name, def) => { const i = args.indexOf(name); return i < 0 ? def : args[i + 1]; };
const repo = resolve(opt('--repo', process.cwd()));
if (!opt('--scratch', process.env.TC_SCRATCH)) throw new Error('--scratch <directory> is required');
const scratch = resolve(opt('--scratch', process.env.TC_SCRATCH));
const out = resolve(opt('--out', join(scratch, 'weather-transitions.json')));
const target = join(repo, 'unreal/Source/ThreeCrossroads/World/TCAtmosphere.TransitionOracle.inl');
const ref = join(repo, 'reference/ww2-rts');
const { installShims, settle } = await import(pathToFileURL(join(repo, 'unreal/Tools/models/shims.mjs')));
installShims();
globalThis.localStorage = { getItem: () => null, setItem: () => {} };
globalThis.location = { search: '' };
const THREE = await import('three');
const imp = (p) => import(pathToFileURL(join(ref, p)));
const { gfx } = await imp('client/gfx.js');
const { MOODS } = await imp('client/moods.js');
const { wind, setWind } = await imp('client/wind.js');
const { mapForClient } = await imp('shared/world-layers.js');
const source = readFileSync(join(ref, 'client/atmosphere.js'), 'utf8');
let edited = source;
const replaceOnce = (old, value) => { assert.equal(edited.split(old).length, 2, `unique source seam: ${old}`); edited = edited.replace(old, value); };
replaceOnce("import { setMood } from './light.js';", 'const setMood = () => {};');
for (const p of ['gfx.js', 'moods.js', 'wind.js']) replaceOnce(`from './${p}'`, `from '${pathToFileURL(join(ref, 'client', p))}'`);
replaceOnce("from '../shared/weather.js'", `from '${pathToFileURL(join(ref, 'shared/weather.js'))}'`);
replaceOnce('    start, update, setWeather,', '    start, update, setWeather,\n    oracleState: () => ({ kind: fall?.userData.kind ?? "", alpha: fallA, wxNow, want: fallKind() ?? "", t }),');
mkdirSync(scratch, { recursive: true });
const modulePath = join(scratch, 'atmosphere-transition-source.mjs');
writeFileSync(modulePath, edited);
const { createAtmosphere } = await import(pathToFileURL(modulePath));
await settle();
const wave = (x, z) => 0.6 * Math.sin(x * 0.13) + 0.4 * Math.cos(z * 0.09) + 0.002 * x - 0.35;
const definitions = [
  { name: 'rain-to-snow-quarter', map: 'pegasus-bridge', start: 'rain', events: [{ count: 12, dt: 0.25, weather: 'snow', wind: [1.2, 0.7] }, { count: 12, dt: 0.25, wind: [-2.5, 1] }] },
  { name: 'shower-stop-restart', map: 'default', start: 'clear', events: [{ count: 1, dt: 0.25, shower: [0.02, 0.2] }, { count: 1, dt: 0.25, shower: [0.020001, 0.2] }, { count: 6, dt: 0.5 }, { count: 12, dt: 0.25, shower: [0, 0.6] }, { count: 1, dt: 0, shower: [0.8, 0.6], wind: [0.4, 0.5] }, { count: 6, dt: 0.5 }] },
  { name: 'desert-dust-rain-fog', map: 'kasserine-pass', start: 'clear', events: [{ count: 12, dt: 0.25, weather: 'rain', wind: [3.1, 0.15] }, { count: 12, dt: 0.25 }, { count: 12, dt: 0.25, weather: 'fog' }, { count: 1, dt: 0, weather: 'clear' }, { count: 6, dt: 0.5 }] },
  { name: 'snow-mud-clear', map: 'bastogne', start: 'snow', events: [{ count: 8, dt: 0.5 }, { count: 24, dt: 0.125, weather: 'mud' }, { count: 1, dt: 0, shower: [0.7, 1] }, { count: 6, dt: 0.5 }, { count: 12, dt: 0.25, weather: 'clear', shower: [0, 0.8] }] },
  { name: 'interrupted-fade', map: 'ardennes-crossing', start: 'rain', events: [{ count: 4, dt: 0.25, weather: 'snow' }, { count: 4, dt: 0.25, weather: 'rain' }, { count: 6, dt: 0.5, weather: 'fog' }, { count: 1, dt: 0, weather: 'snow' }, { count: 3, dt: 1 }] },
];
const cases = [];
for (const graphics of ['high', 'low']) for (const def of definitions) {
  gfx.set(graphics); // Always explicit, including before construction and every lane.
  wind.x = 0.55; wind.z = 0.25;
  const map = mapForClient(JSON.parse(readFileSync(join(ref, 'maps', `${def.map}.json`), 'utf8')));
  const width = map.w * 2, height = map.h * 2;
  const scene = new THREE.Scene();
  const renderer = { getDrawingBufferSize: (v) => v.set(1280, 720), getPixelRatio: () => 1 };
  const camera = new THREE.PerspectiveCamera(42, 16 / 9, 1, 900);
  const at = createAtmosphere({ scene, renderer, camera, cam: { x: width * 0.4, y: 0, z: height * 0.55 } });
  const ground = new THREE.Mesh(new THREE.PlaneGeometry(width, height));
  const apron = { overlayGeometry: new THREE.BufferGeometry(), mesh: { geometry: new THREE.BufferGeometry() } };
  at.start({ map, key: def.map, ground, hAt: wave, weather: [def.start], apron });
  const root = scene.children.find(o => o.name === 'atmosphere');
  const [cloud, , cover, , mist, banks, birds] = root.children;
  let rain = 0, wet = 0;
  const frames = [];
  const record = (dt) => {
    const s = at.oracleState();
    const fall = root.children.find(o => o.isPoints);
    frames.push({ dt, weather: s.wxNow, rain, wet, want: s.want,
      kind: s.kind, alpha: s.alpha, time: s.t,
      fallVisible: !!fall?.visible, drawn: fall ? fall.geometry.drawRange.count : 0,
      velocity: fall ? fall.material.uniforms.uVel.value.toArray() : [0, 0, 0],
      visible: { cloud: cloud.visible, cover: cover.visible, mist: mist.visible, banks: banks.visible, birds: birds.visible },
      counts: { mist: mist.count, banks: banks.count },
      cover: Object.fromEntries(['Wet', 'Sheen', 'Mud', 'Cover'].map(k => [k.toLowerCase(), cover.material.uniforms[`u${k}`].value])),
      bankOpacity: banks.material.uniforms.uOpacity.value });
  };
  record(0);
  for (const event of def.events) {
    if (event.weather) at.setWeather([event.weather]);
    if (event.wind) setWind(...event.wind);
    if (event.shower) { [rain, wet] = event.shower; at.setRain(rain, wet); }
    for (let i = 0; i < event.count; i++) { at.update(event.dt); record(event.dt); }
  }
  // The source uses the whole current ground geometry, including changed normals
  // and triangle contents when the vertex positions and buffer sizes stay equal.
  const before = ground.geometry;
  const normalsOnly = before.clone();
  normalsOnly.attributes.normal.setXYZ(0, 0, 0.6, 0.8);
  ground.geometry = normalsOnly; at.update(0);
  assert.equal(cover.geometry, normalsOnly); assert.equal(cloud.geometry, normalsOnly);
  assert.deepEqual(cover.geometry.attributes.position.array, before.attributes.position.array);
  const indicesOnly = normalsOnly.clone();
  indicesOnly.index.set([0, 2, 3, 0, 3, 1]);
  ground.geometry = indicesOnly; at.update(0);
  assert.equal(cover.geometry, indicesOnly); assert.equal(cloud.geometry, indicesOnly);
  assert.equal(indicesOnly.index.count, before.index.count);
  const groundReplacements = { normalOnlyShared: true, sameCountIndicesShared: true, positionsUnchanged: true };
  cases.push({ name: `${def.name}-${graphics}`, map: def.map, mood: at.mood, moodWeather: MOODS[at.mood].weather ?? '', graphics, start: def.start, frames, groundReplacements });
}
const main = cases.find(c => c.name === 'rain-to-snow-quarter-high');
assert.equal(main.frames[11].kind, 'rain');
assert.equal(main.frames[12].kind, 'snow');
assert.equal(main.frames[12].alpha, 0);
for (const c of cases) for (const f of c.frames) {
  assert.equal(f.want, ['rain', 'snow'].includes(f.weather) ? f.weather : f.rain > 0.02 ? 'rain' : c.moodWeather === 'dust' && f.weather === 'clear' ? 'dust' : '');
}
const sources = Object.fromEntries(['client/atmosphere.js', 'client/gfx.js', 'client/wind.js', 'client/moods.js', 'shared/weather.js'].map(p => [p, createHash('sha256').update(readFileSync(join(ref, p))).digest('hex')]));
const fixture = { format: 'tc-weather-transitions-v1', reference: 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d', sources, cases };
mkdirSync(resolve(out, '..'), { recursive: true });
writeFileSync(out, JSON.stringify(fixture, null, 2) + '\n');
const quote = JSON.stringify;
const number = n => Number.isInteger(n) ? `${n}.0` : `${n}`;
let table = '// Generated from the pinned atmosphere.js update by Tools/atmosphere/transition-oracle.mjs.\n// Each case restarts in its source fall kind and alpha. No runtime RNG is changed.\n';
table += 'struct FWeatherTransitionOracle { int Case; double Dt; const char* Weather; double Rain; const char* MoodWeather; const char* Want; const char* Kind; double Alpha; };\n';
table += 'static const FWeatherTransitionOracle WeatherTransitionOracle[] = {\n';
cases.forEach((c, index) => c.frames.forEach((f, frame) => {
  table += `    { ${frame === 0 ? index : -1}, ${number(f.dt)}, ${quote(f.weather)}, ${number(f.rain)}, ${quote(c.moodWeather === 'dust' ? 'dust' : '')}, ${quote(f.want)}, ${quote(f.kind)}, ${number(f.alpha)} },\n`;
}));
table += '};\n';
if (args.includes('--check')) assert.equal(readFileSync(target, 'utf8'), table, 'generated weather fixture is current');
else writeFileSync(target, table);
console.log(`weather transitions: ${cases.length} source sequences, ${cases.reduce((n, c) => n + c.frames.length, 0)} frames, ${args.includes('--check') ? 'fixture current' : 'fixture written'}`);
