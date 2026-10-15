// Execute original walk, recipient route and live deferred-goal owners together.
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const reference = resolve(process.env.WW2_REFERENCE ?? fileURLToPath(new URL('../reference/ww2-rts/', import.meta.url)));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => {
  if (git(['rev-parse', 'HEAD']) !== pin || git(['status', '--porcelain', '--untracked-files=no'])) throw Error('Clean pinned Source required');
};
if (process.arch !== 'x64' || process.version !== 'v24.21.0') throw Error('Use x64 Node v24.21.0');
clean();
const suffix = '\nexport {walkRoute as __walkRoute,walkable as __walkable,knownTerrainFor as __knownTerrainFor,beginPathTick as __beginPathTick,requestStepPath as __requestStepPath,placeUnit as __placeUnit};\n';
registerHooks({ load(url, context, next) {
  const result = next(url, context);
  return url === pathToFileURL(resolve(reference, 'shared/sim.js')).href
    ? { ...result, source: String(result.source) + suffix } : result;
} });
const sim = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const dv = new DataView(new ArrayBuffer(8));
const bits = n => { dv.setFloat64(0, n); return dv.getBigUint64(0).toString(16).padStart(16, '0'); };
const pointBits = p => p ? [bits(p.x), bits(p.z)] : null;
const at = (x, z) => ({ x, z });
const map = (name, rows, extra = {}) => ({ name, w: rows[0].length, h: rows.length, rows,
  spawns: [{ x: 2, y: 2 }, { x: rows[0].length - 3, y: rows.length - 3 }],
  points: [{ id: 'component', x: 12, y: 12 }], ...extra });
const flat = Array(20).fill('.'.repeat(40));
const ocean = Array(20).fill('W'.repeat(40)).map((row, y) => {
  const x = y === 2 ? 2 : y === 17 ? 37 : -1;
  return x < 0 ? row : row.slice(0, x) + '.' + row.slice(x + 1);
});
const maps = {
  flat: map('wrapper-flat', flat),
  wall: map('wrapper-wall', flat.map(row => row.slice(0, 20) + 'B' + row.slice(21))),
  bank: map('wrapper-bank', flat.map((row, y) => y === 8 ? 'W'.repeat(40) : row)),
  naval: map('wrapper-naval', ocean, { naval: true, points: [{ id: 'component', x: 2, y: 2 }] }),
};
const cases = [];
const projection = g => [...g.units.values()].map(u => ({ id: u.id, x: bits(u.x), z: bits(u.z), rot: bits(u.rot),
  path: u.path.map(pointBits), repath: bits(u.repath), stuck: bits(u.stuck), worldGoal: pointBits(u.worldGoal),
  amove: pointBits(u.amove), routeEnd: pointBits(u.routeEnd), outcome: u.moveOutcome ?? null,
  outcomeTick: u.moveOutcomeTick ?? null }));
function record(name, terrain, initial, actions) {
  const invalid = sim.validateMap(terrain);
  if (invalid !== null) throw Error(`Invalid map ${name}: ${invalid}`);
  const saved = Math.random;
  Math.random = () => 0.5;
  let g, players;
  try {
    g = sim.createGame(terrain, [], false, [], [], { weather: false });
    players = sim.createGame(terrain, ['A', 'B'], false, [0, 1], [0, 1], { weather: false }).players;
  } finally { Math.random = saved; }
  g.players = players; g.units = new Map(); g.nextId = 1; g.tick = 1;
  g.supply = false; g.logistics = false; g.winner = null;
  for (const row of initial) {
    const u = sim.internals.spawnUnit(g, row.owner ?? 0, row.kind);
    Object.assign(u, { x: row.x, z: row.z, holdFire: true, auto: false, autoRetreat: false, ...structuredClone(row.fields) });
    sim.__placeUnit(g, u, u);
  }
  for (const p of players) p.visible = new Set();
  const steps = [];
  for (const action of actions) {
    const input = structuredClone(action);
    let result = null;
    const u = g.units.get(action.id ?? 1);
    if (action.tick !== undefined) g.tick = action.tick;
    switch (action.op) {
      case 'set': Object.assign(u, action.fields); sim.__placeUnit(g, u, u); break;
      case 'walk': {
        const route = u.path;
        sim.__walkRoute(g, u, sim.UNITS[u.type], route, sim.UNITS[u.type].speed, sim.TICK,
          action.back ?? false, { yielding: false }, () => sim.__knownTerrainFor(g, u.owner));
        break;
      }
      case 'clear': result = sim.__walkable(g, action.a, action.b, action.mask ?? sim.MOVE, action.half ?? 0.9); break;
      case 'begin':
        g.pathBudget = action.budget;
        g.pathRetryLimit = action.retry;
        sim.__beginPathTick(g); break;
      case 'request': sim.__requestStepPath(g, u, action.to, action.kind); break;
      case 'command': result = sim.command(g, action.owner ?? 0, action.command) ?? null; break;
      default: throw Error(`Unknown action ${action.op}`);
    }
    steps.push({ action: input, result, units: projection(g) });
  }
  cases.push({ name, map: terrain, initial, steps });
}
const unit = (kind = 'rifle', x = 9, z = 19, fields = {}) => ({ kind, x, z, fields });
for (const kind of ['rifle', 'engineer', 'lcvp', 'gunboat', 'destroyer']) {
  const terrain = sim.UNITS[kind].naval ? maps.naval : maps.flat;
  // Boats stay over 10 cells from both isolated land pads, including deep hulls.
  const x = sim.UNITS[kind].naval ? 39 : 9;
  record(`walk-budget-${kind}`, terrain, [unit(kind, x, 19)], [
    { op: 'set', fields: { path: [at(x, 19), at(x + 0.04, 19), at(x + 0.08, 19), at(x + 2, 19)], repath: 0 } },
    { op: 'walk' },
    { op: 'set', fields: { path: [at(x + 2, 19)], reverse: at(x + 2, 19), repath: 0 } },
    { op: 'walk', back: true },
  ]);
}
for (const distance of [0.999, 1, 1.001]) record(`waypoint-threshold-${distance}`, maps.flat,
  [unit('rifle', 9, 19, { path: [at(9 + distance, 19), at(15, 19)] })], [{ op: 'walk' }]);
record('walk-blocked-bank', maps.bank, [unit('rifle', 9, 15.95, { path: [at(9, 19)] })], [{ op: 'walk' }]);

const rays = [];
for (const [a, b] of [[at(2, 2), at(4, 4)], [at(4, 2), at(2, 4)], [at(2, 4), at(4, 2)],
  [at(4, 4), at(2, 2)], [at(9, 15), at(9, 17)], [at(9, 17), at(9, 15)],
  [at(9, 19), at(9, 19)], [at(-1, 19), at(9, 19)]]) {
  for (const half of [0, 0.9, 2]) rays.push({ op: 'clear', a, b, half });
}
record('ray-endpoints-and-borders', maps.bank, [unit()], rays);
for (const x of [8.05, 8.5, 9, 9.5, 9.95]) record(`remembered-bank-${x}`, maps.bank,
  [unit('rifle', x, 18.05, { holdPos: true })],
  [{ op: 'command', command: { t: 'move', orders: [[1, 61, 19]] } }]);

for (const [kind, values] of [['world', [0.249, 0.25, 0.251]], ['recovery', [0.749, 0.75, 0.751]], ['amove', [2.499, 2.5, 2.501]]]) {
  for (const value of values) {
    const goal = kind === 'recovery' ? at(21, 19) : at(9 + value, 19);
    const fields = kind === 'recovery' ? { path: [goal], stuck: value } : { path: [], [kind === 'world' ? 'worldGoal' : 'amove']: goal };
    record(`live-${kind}-eligibility-${value}`, maps.flat, [unit('rifle', 9, 19, fields)], [
      { op: 'begin', tick: 1, budget: 0 }, { op: 'request', kind, to: goal },
      { op: 'begin', tick: 2, budget: 1 },
    ]);
  }
}
record('default-twenty-real-failures', maps.wall, [unit('rifle', 9, 19, { worldGoal: at(71, 19) })],
  Array.from({ length: 20 }, (_, i) => [
    { op: 'begin', tick: i + 1 }, { op: 'request', kind: 'world', to: at(71, 19) },
  ]).flat());
record('real-fifo-stale-dead-and-replacement', maps.flat, [unit(), unit('rifle', 11, 21)], [
  { op: 'set', id: 1, fields: { path: [], worldGoal: at(31, 19) } },
  { op: 'set', id: 2, fields: { path: [], worldGoal: at(33, 21) } },
  { op: 'begin', tick: 1, budget: 0 },
  { op: 'request', id: 1, kind: 'world', to: at(31, 19) },
  { op: 'request', id: 2, kind: 'world', to: at(33, 21) },
  { op: 'set', id: 1, fields: { worldGoal: at(35, 19) } },
  { op: 'request', id: 1, kind: 'world', to: at(35, 19) },
  { op: 'begin', tick: 2, budget: 1 },
  { op: 'begin', tick: 3, budget: 1 },
  { op: 'set', id: 1, fields: { path: [], worldGoal: at(39, 19) } },
  { op: 'begin', tick: 4, budget: 0 },
  { op: 'request', id: 1, kind: 'world', to: at(39, 19) },
  { op: 'set', id: 1, fields: { hp: 0 } }, { op: 'begin', tick: 5, budget: 0 },
]);

clean();
const failures = cases.find(c => c.name === 'default-twenty-real-failures');
if (failures.steps.at(-3).units[0].outcome === 'unreachable' || failures.steps.at(-1).units[0].outcome !== 'unreachable') {
  throw Error('The actual default retry owner did not drop exactly on failure twenty');
}
const bankFallback = cases.filter(c => c.name.startsWith('remembered-bank-')).some(c => {
  const u = c.initial[0], first = c.steps[0].units[0].path[0];
  return first && first[0] === bits(u.x) && first[1] === bits(19);
});
if (!bankFallback) throw Error('No ordinary remembered-view bank fallback was witnessed');
const output = JSON.stringify({ reference: pin, node: process.version, arch: process.arch, sourceExportSuffix: suffix,
  rows: ['MOV-11', 'MOV-12', 'MOV-13', 'MOV-30', 'MOV-40', 'MOV-41'], cases }) + '\n';
const target = new URL('../rust/crates/core/tests/fixtures/mov-wrapper-batch.json', import.meta.url);
if (process.argv.includes('--check')) {
  if (readFileSync(target, 'utf8') !== output) throw Error('MOV wrapper fixture differs');
} else writeFileSync(target, output);
console.log(JSON.stringify({ cases: cases.length, actions: cases.reduce((n, c) => n + c.steps.length, 0) }));
