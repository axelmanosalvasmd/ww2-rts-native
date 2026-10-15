// Named branch witnesses for the existing fine-search component, not a match replay.
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { registerHooks } from 'node:module';

const reference = resolve(process.env.WW2_REFERENCE ?? fileURLToPath(new URL('../reference/ww2-rts/', import.meta.url)));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => {
  if (git(['rev-parse', 'HEAD']) !== pin || git(['status', '--porcelain', '--untracked-files=no'])) throw Error('Clean pinned reference required');
};
if (process.arch !== 'x64' || process.version !== 'v24.21.0') throw Error('Use x64 Node v24.21.0');
clean();
registerHooks({ load(url, context, next) {
  const result = next(url, context);
  if (url !== pathToFileURL(resolve(reference, 'shared/sim.js')).href) return result;
  return { ...result, source: String(result.source) + '\nexport { nearestFree as __nearestFree, cellOf as __cellOf };\n' };
} });
const sim = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const motion = await import(pathToFileURL(resolve(reference, 'shared/vehicle-motion.js')));
const { navigationLeg } = await import(pathToFileURL(resolve(reference, 'shared/navigation.js')));
const data = new DataView(new ArrayBuffer(8));
const bits = n => { data.setFloat64(0, n); return data.getBigUint64(0).toString(16).padStart(16, '0'); };
const pt = (x, z) => ({ x, z });
const pointBits = p => [bits(p.x), bits(p.z)];
const runs = values => {
  const out = [];
  for (const v of values) {
    if (out.length && out.at(-1)[0] === v) out.at(-1)[1]++;
    else out.push([v, 1]);
  }
  return out;
};
const infantry = ['rifle', 'engineer', 'sniper', 'mg', 'at'];
const naval = ['lcvp', 'gunboat', 'destroyer'];
const vehicles = Object.keys(sim.UNITS).filter(type => {
  const d = sim.UNITS[type];
  return !d.infantry && !d.naval && !d.air && !d.rail && !d.structure;
});
const profiles = [...infantry, ...naval, ...vehicles];
const scenarios = [];
function scenario(name, rows, w, h, flags, heights, options = {}) {
  const near = new Uint8Array(w * h);
  for (let c = 0; c < flags.length; c++) if ((flags[c] & (sim.MOVE | sim.SIGHT)) === (sim.MOVE | sim.SIGHT)) {
    const x = c % w;
    for (let dy = -w; dy <= w; dy += w) for (let dx = x > 0 ? -1 : 0; dx <= (x < w - 1 ? 1 : 0); dx++) {
      const n = c + dy + dx;
      if (n >= 0 && n < near.length) near[n] = 1;
    }
  }
  const { mines = [], fires = [], bunkers = [], wear = null, wetness = 0, roads = false, worldKnown = false } = options;
  const g = { w, h, flags: Uint16Array.from(flags), height: heights && Int8Array.from(heights), navigationObserved: true,
    worldKnown, worldNearWalls: near, wear: wear && Float32Array.from(wear), wx: { wet: wetness }, roads,
    players: [{ team: 0 }], mineSeen: new Map(mines.map(c => [c, 1])), fires: new Map(fires.map(c => [c, 20])),
    units: new Map(bunkers.map((p, i) => [i + 1, { ...p, type: 'bunker', hp: 100 }])),
    infantryRegionVersion: 0, vehicleRegionVersion: 0, navalRegionVersion: 0, navigationKnowledgeVersion: 0 };
  const record = { name, rows, w, h, flags: runs(flags), height: heights && runs(heights), near: runs(near),
    worldKnown, mines, fires, bunkers, wear: wear && runs(Float32Array.from(wear)), wetness, roads, queries: [] };
  scenarios.push(record);
  return (type, from, to, limit = null) => {
    g.pathStats = { calls: 0, deferred: 0, failed: 0, dropped: 0, expansions: 0, regionRejected: 0 };
    g.finePathLimit = limit ?? undefined;
    const unit = { id: 999, owner: 0, type, ...from };
    const def = sim.UNITS[type];
    const view = motion.isGroundVehicle(def) ? motion.vehicleNavigationView(g, def, sim.blockOf(def)) : g;
    view.pathStats = g.pathStats;
    view.finePathLimit = g.finePathLimit;
    const block = sim.blockOf(def), fullGoal = sim.__nearestFree(view, to.x, to.z, block);
    const stand = Math.max(0, sim.__cellOf(view, from.x, from.z));
    const start = view.flags[stand] & block ? sim.__nearestFree(view, from.x, from.z, block) : stand;
    const leg = navigationLeg(view, start, fullGoal, block);
    const path = sim.findPath(view, unit, to);
    const expected = { points: path.map(pointBits), routeEnd: pointBits(unit.routeEnd), expansions: g.pathStats.expansions,
      status: g.pathStats.deferred ? 'ExpansionLimit' : g.pathStats.failed ? 'Unreachable' : 'Found',
      localGoal: leg.goal, limited: leg.limited };
    record.queries.push({ type, from, to, limit, expected });
    return expected;
  };
}

// Directional cliffs and both blocked corners, with and without known mines.
for (const mines of [[], [1], [1, 2]]) {
  const q = scenario(`diagonal-corners-mines-${mines.length}`, ['MOV-31'], 2, 2, [0, 0, 0, 0], [0, 2, 2, 1], { mines });
  for (const type of infantry) { q(type, pt(3, 3), pt(1, 1)); q(type, pt(1, 1), pt(3, 3)); }
}

// Two equal detours expose heap ties. Repeated calls retain scratch generations.
const w = 25, h = 19, wall = Array(w * h).fill(0);
for (let y = 4; y < 15; y++) wall[y * w + 12] = sim.MOVE | sim.SIGHT;
for (const roads of [false, true]) {
  const flags = wall.slice();
  if (roads) for (let x = 2; x < 23; x++) flags[2 * w + x] = sim.ROAD;
  const q = scenario(`equal-detours-roads-${roads}`, ['MOV-31', 'MOV-33', 'MOV-35'], w, h, flags, null, { roads });
  for (const type of [...infantry, ...vehicles]) for (let repeat = 0; repeat < 2; repeat++) q(type, pt(7, 19), pt(43, 19));
}

// Each cost competes with a longer open detour, including submerged wet ground.
for (const [name, flag, opts] of [
  ['wire', sim.WIRE, {}], ['fire', 0, { fires: [235, 236, 237, 238, 239] }],
  ['mine', 0, { mines: [235, 236, 237, 238, 239] }], ['bunker', 0, { bunkers: [pt(25, 19)] }],
  ['wood', sim.WOOD, {}], ['mud', sim.MUD, { wetness: 0.8 }],
  ['ford', sim.FORD, { wetness: 0.8 }], ['road', sim.ROAD, { roads: true }],
  ['worn-open', 0, { wetness: 0.8 }], ['uphill', 0, {}],
]) {
  const flags = Array(w * h).fill(0), height = Array(w * h).fill(0), wear = Array(w * h).fill(0);
  for (let y = 7; y <= 11; y++) for (let x = 9; x <= 15; x++) {
    flags[y * w + x] = flag;
    wear[y * w + x] = name === 'road' ? 0.65 : 0.8;
    height[y * w + x] = name === 'uphill' ? 1 : -1;
  }
  const q = scenario(`cost-${name}`, ['MOV-32', 'MOV-35'], w, h, flags, height, { wear, ...opts });
  for (const type of [...infantry, ...vehicles]) q(type, pt(7, 19), pt(43, 19));
}

// Goal-cell and blocked-start branches, including the same-cell escape path.
for (const blocked of [false, true]) {
  const flags = Array(w * h).fill(0);
  if (blocked) flags[9 * w + 3] = sim.MOVE | sim.SIGHT;
  const q = scenario(`goal-start-${blocked}`, ['MOV-34'], w, h, flags, null);
  for (const type of [...infantry, ...vehicles]) {
    q(type, pt(7, 19), pt(7, 19)); q(type, pt(7, 19), pt(7.6, 19.4));
    q(type, pt(7, 19), pt(43.7, 19.2)); q(type, pt(7, 19), pt(-3, 80));
  }
}

// Naval pull masks and 2 m clearance, with a land peninsula or shoal strip.
for (const obstruction of [sim.LAND, sim.SHOAL]) {
  const flags = Array(w * h).fill(sim.MOVE);
  for (let y = 5; y < 14; y++) flags[y * w + 12] |= obstruction;
  const q = scenario(`naval-pull-${obstruction}`, ['MOV-31', 'MOV-34', 'MOV-35'], w, h, flags, null);
  for (const type of naval) { q(type, pt(7, 19), pt(43, 19)); q(type, pt(7, 19), pt(25, 19)); }
}

// Full small-map routes versus bounded legs at the size and World boundaries.
for (const width of [128, 129]) for (const worldKnown of [false, true]) {
  const q = scenario(`leg-${width}-${worldKnown}`, ['MOV-38'], width, 9, Array(width * 9).fill(0), null, { worldKnown });
  for (const type of [...infantry, ...vehicles]) q(type, pt(9, 9), pt(width * 2 - 9.2, 9));
}

// A known mine blocks string-pulling while a fine route still exists.
const budget = scenario('expansion-boundaries', ['MOV-31', 'MOV-39'], w, h, wall, null, { mines: [9 * w + 8] });
for (const type of [...infantry, ...vehicles]) {
  const complete = budget(type, pt(7, 19), pt(43, 19));
  for (const limit of [0, 1, 8, 32, Math.max(0, complete.expansions - 1), complete.expansions]) budget(type, pt(7, 19), pt(43, 19), limit);
}

clean();
const counts = { queries: 0, found: 0, unreachable: 0, deferred: 0 };
for (const s of scenarios) for (const q of s.queries) {
  counts.queries++;
  counts[q.expected.status === 'Found' ? 'found' : q.expected.status === 'Unreachable' ? 'unreachable' : 'deferred']++;
}
const output = JSON.stringify({ reference: pin, node: process.version, arch: process.arch, counts,
  rules: { wallHug: sim.CFG.wallHug, bunkerRadius: sim.UNITS.bunker.radius, roadSpeed: sim.CFG.roadSpeed,
    fordSpeed: sim.CFG.fordSpeed, mudSpeed: sim.CFG.mudSpeed, woodSpeed: sim.CFG.wood.speed, churn: sim.CFG.churn,
    wetFord: sim.CFG.weather.wetFord, wetGround: sim.CFG.weather.wetGround },
  profiles: Object.fromEntries(profiles.map(type => { const d = sim.UNITS[type]; return [type,
    { infantry: !!d.infantry, naval: !!d.naval, deep: !!d.deep, radius: d.radius, speed: d.speed }]; })), scenarios }) + '\n';
const target = new URL('../rust/crates/simtest/tests/fixtures/pathfinding-branches.json', import.meta.url);
if (process.argv.includes('--check')) {
  if (readFileSync(target, 'utf8') !== output) throw Error('Pathfinding branch fixture differs');
} else writeFileSync(target, output);
console.log(JSON.stringify({ scenarios: scenarios.length, profiles: profiles.length, ...counts }));
