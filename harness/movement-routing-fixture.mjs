// Records live fine-search recovery inside the existing frozen movement workload.
// Other workload exclusions remain: no combat, terrain mutation, logistics or hidden views.
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { trace, traceSimWithRoutes } from './movement-trace.mjs';
if (process.arch !== 'x64' || process.version !== 'v24.21.0') throw Error('Use x64 Node v24.21.0');
const reference = fileURLToPath(new URL('../reference/ww2-rts/', import.meta.url));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
if (git(['rev-parse', 'HEAD']) !== pin || git(['status', '--porcelain', '--untracked-files=no'])) throw Error('Reference must be clean at the named pin');
const sim = await traceSimWithRoutes();
const cases = [];
for (const [kind, blocked] of [['rifle', false], ['tank', false], ['rifle', true], ['tank', true]]) {
  const w = 32, h = 24;
  const rows = Array.from({ length: h }, (_, y) => Array.from({ length: w }, (_, x) => x === 16 && (blocked || y < 4 || y > 8) ? 'B' : '.').join(''));
  const map = { w, h, rows, spawns: [{ x: 3, y: 11 }, { x: 28, y: 11 }], points: [] };
  const random = Math.random;
  Math.random = () => 0.375;
  let g;
  try { g = sim.createGame(map, ['A', 'B'], false, [0, 1], [0, 1], { weather: false }); }
  finally { Math.random = random; }
  const template = structuredClone(g.units.values().next().value);
  const def = sim.UNITS[kind];
  const goal = { x: 55, z: 23 };
  const u = Object.assign(template, { id: 1, type: kind, owner: 0, x: 9, z: 23, rot: 0, hp: def.models * def.hpPer,
    path: [goal], orders: [], attackId: 0, targetId: 0, holdFire: true, auto: false, autoRetreat: false,
    stuck: 1, repath: 0, worldGoal: null, amove: null, pace: undefined, ghost: undefined,
    logistics: undefined, convoy: undefined, air: undefined, rail: undefined });
  g.units = new Map([[u.id, u]]); g.nextId = 2;
  g.navigationObserved = true; // Explicitly selected terrain, without recipient-view inference.
  g.worldNearWalls = new Uint8Array(w * h);
  for (let c = 0; c < g.flags.length; c++) if ((g.flags[c] & (sim.MOVE | sim.SIGHT)) === (sim.MOVE | sim.SIGHT)) {
    const x = c % w;
    for (let dy = -w; dy <= w; dy += w) for (let dx = x > 0 ? -1 : 0; dx <= (x < w - 1 ? 1 : 0); dx++) {
      const n = c + dy + dx; if (n >= 0 && n < g.flags.length) g.worldNearWalls[n] = 1;
    }
  }
  Object.assign(g, { shots: [], nades: [], salvos: [], strikes: [], smokes: [], newCells: [], fallingSections: [] });
  g.flights.clear(); g.fires.clear();
  const recording = await trace(sim, g, 12);
  const changed = recording.steps.some(s => s.events.some(e => e[0] === 'move' && e[4]?.path));
  if (!blocked && !changed) throw Error(`${kind}: recovery did not install a new path`);
  cases.push({ name: `${kind}-${blocked ? 'unreachable' : 'reroute'}`, ...recording });
}
const output = JSON.stringify({ reference: pin, node: process.version, arch: process.arch,
  rules: { wallHug: sim.CFG.wallHug, bunkerRadius: sim.UNITS.bunker.radius, roadSpeed: sim.CFG.roadSpeed,
    woodSpeed: 0.5, fordSpeed: sim.CFG.fordSpeed, mudSpeed: sim.CFG.mudSpeed, churn: sim.CFG.churn,
    wetFord: sim.CFG.weather.wetFord, wetGround: sim.CFG.weather.wetGround }, cases }) + '\n';
const path = fileURLToPath(new URL('../rust/crates/simtest/tests/fixtures/movement-routing.json', import.meta.url));
if (process.argv.includes('--check')) {
  if (readFileSync(path, 'utf8') !== output) throw Error('Movement-routing fixture differs');
} else writeFileSync(path, output);
console.log(`Verified ${cases.length} live-routing cases, ${cases.reduce((n, c) => n + c.steps.length, 0)} ticks`);
