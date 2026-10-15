// Original selected route checks. Source bodies stay unchanged; exports expose their actual routing view.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

if (process.arch !== 'x64' || process.version !== 'v24.21.0') throw Error('Use x64 Node v24.21.0');
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
registerHooks({ load(url, context, next) {
  const result = next(url, context);
  if (url !== pathToFileURL(resolve(reference, 'shared/sim.js')).href) return result;
  return { ...result, source: String(result.source) + '\nexport { nearestFree as __nearestFree, cellOf as __cellOf, nearWalls as __nearWalls, knownTerrainFor as __knownTerrainFor };\n' };
} });
const sim = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const motion = await import(pathToFileURL(resolve(reference, 'shared/vehicle-motion.js')));
const { navigationLeg } = await import(pathToFileURL(resolve(reference, 'shared/navigation.js')));
const { clearFixtureUnits } = await import(pathToFileURL(resolve(reference, 'test-fixtures.js')));
const oracle = {
  house: '603d83d3454052aeaa916ffcc52c55f0e68c9d97c4467121382ebb8cffd06321',
  mines: '595d74b9da8cd4806c442ce462b48c635c935ad6e1d0d643af779e4a5be172f0',
  roadMud: 'f3214194663892841cb096763ea9d9732a72149237fbeeef7f38df7a7c819dba',
  naval: 'f6dfa6f44a61afe78340cb771aee1b8acb537171b3202b70ac5f80f54f0229e0',
  elevation: '2c7348829bdd69db854f3ff21874901818b9540c92b4a43bcb1dff9e763a208e',
  bunkerBase: 'ca40f1b77e8940d39a59c4d3227e14af5c409fcd293c061eb7191d0b0e415b59',
  bunkerNear: '9f197446e69527df75469b8a595ddbb2b546f1fd0bc4d186c8970599c5fcd170',
  freshRoad: '357795d0df4e3151ec226ac54e3740cea85d12d51403df91c6c67446bbd98151',
  wornRoad: 'ebc84399dc4d07d167c45b883dabfb8d941990c9729e34f5272a997b92bf54f7',
};
const hash = v => createHash('sha256').update(JSON.stringify(v)).digest('hex');
const data = new DataView(new ArrayBuffer(8));
const bits = n => { data.setFloat64(0, n); return data.getBigUint64(0).toString(16).padStart(16, '0'); };
const pointBits = p => [bits(p.x), bits(p.z)];
const runs = a => {
  const out = [];
  for (const n of a) { if (out.length && Object.is(out.at(-1)[0], n)) out.at(-1)[1]++; else out.push([n, 1]); }
  return out;
};
function plain(v) {
  if (v instanceof Map) return [...v].map(([k, value]) => [k, plain(value)]);
  if (v instanceof Set || ArrayBuffer.isView(v)) return [...v].map(plain);
  if (Array.isArray(v)) return v.map(plain);
  if (v && typeof v === 'object') return Object.fromEntries(Object.entries(v).map(([k, value]) => [k, plain(value)]));
  return typeof v === 'number' && !Number.isFinite(v) ? { floatBits: bits(v) } : v;
}
let startupState = 0x4d4f5631, startupDraws = 0;
function startupRandom() {
  startupState = (startupState + 0x6d2b79f5) >>> 0;
  let t = startupState;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  startupDraws++;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}
function create(map, options, clear = null) {
  const prior = Math.random, before = startupDraws;
  let g;
  Math.random = startupRandom;
  try { g = sim.createGame(map, ['a', 'b'], false, [0, 1], [0, 1], options); }
  finally { Math.random = prior; }
  if (clear === 'buildings') clearFixtureUnits(g, u => sim.UNITS[u.type].building);
  if (clear === 'all') {
    clearFixtureUnits(g);
    g.players.forEach(p => { p.spawn = { x: -1000, z: -1000 }; });
  }
  return { g, provenance: { map, options, clear, startupDraws: startupDraws - before,
    actors: plain(g.units), structures: plain(g.structures ?? new Map()),
    structuralCells: plain(g.structuralCells ?? new Map()), buildingCells: plain(g.buildingCells ?? new Map()) } };
}
const scenes = [];
function scene(name, created) {
  const entry = { name, provenance: created.provenance, queries: [] };
  scenes.push(entry);
  const g = created.g;
  return { g, query(type, from, to, checks = {}) {
    const def = sim.UNITS[type], unit = { ...from, type };
    const base = !g.navigationObserved && !g.worldKnown && g.players?.[unit.owner] ? sim.__knownTerrainFor(g, unit.owner) : g;
    const view = motion.isGroundVehicle(def) ? motion.vehicleNavigationView(base, def, sim.blockOf(def)) : base;
    const before = { expansions: view.pathStats?.expansions ?? 0, failed: view.pathStats?.failed ?? 0, deferred: view.pathStats?.deferred ?? 0 };
    const points = sim.findPath(g, unit, to);
    const block = sim.blockOf(def), goal = sim.__nearestFree(view, to.x, to.z, block);
    const stand = Math.max(0, sim.__cellOf(view, from.x, from.z));
    const start = view.flags[stand] & block ? sim.__nearestFree(view, from.x, from.z, block) : stand;
    const leg = navigationLeg(view, start, goal, block);
    const team = base.players?.[unit.owner]?.team;
    const knownMines = [...(base.mineSeen ?? new Map())].filter(([, seen]) => team !== undefined && (seen >> team & 1)).map(([c]) => c);
    const bunkers = [...(base.units?.values() ?? [])].filter(u => u.type === 'bunker' && u.hp > 0).map(u => ({ x: u.x, z: u.z }));
    const routeEnd = unit.routeEnd ?? (goal === sim.__cellOf(view, to.x, to.z) && (!motion.isGroundVehicle(def) || motion.vehicleDestinationClear(view, to, def, block)) ? to : { x: (goal % g.w + .5) * sim.CELL, z: (Math.floor(goal / g.w) + .5) * sim.CELL });
    const coordinates = points.map(p => [p.x, p.z]);
    const terrain = { flags: runs(base.flags), chars: runs(base.chars), height: base.height && runs(base.height), near: runs(sim.__nearWalls(view)),
      wear: base.wear && runs(base.wear), wetness: base.wx?.wet ?? 0, roads: !!base.roads, worldKnown: !!base.worldKnown,
      versions: { infantry: base.infantryRegionVersion ?? 0, vehicle: base.vehicleRegionVersion ?? 0, naval: base.navalRegionVersion ?? 0, obstruction: base.obstructionVersion ?? 0, knowledge: base.navigationKnowledgeVersion ?? 0 },
      mines: knownMines, fires: [...(base.fires?.keys() ?? [])], bunkers,
      navigationFlags: runs(view.flags), navigationHeight: view.height && runs(view.height) };
    const expected = { points: coordinates, pointBits: points.map(pointBits), routeJson: JSON.stringify(coordinates), routeEnd: pointBits(routeEnd),
      expansions: (view.pathStats?.expansions ?? 0) - before.expansions,
      status: (view.pathStats?.deferred ?? 0) > before.deferred ? 'ExpansionLimit' : (view.pathStats?.failed ?? 0) > before.failed ? 'Unreachable' : 'Found',
      connected: leg.connected, start, fullGoal: goal, localGoal: leg.goal, limited: leg.limited };
    if (checks.hash) assert.equal(hash(coordinates), oracle[checks.hash], checks.hash);
    if (checks.hull) {
      assert.ok(points.length, 'a vehicle has a reachable route');
      let at = from;
      for (const p of points) {
        const d = Math.hypot(p.x - at.x, p.z - at.z), rot = Math.atan2(p.z - at.z, p.x - at.x), n = Math.max(1, Math.ceil(d / .25));
        for (let i = 0; i <= n; i++) assert.ok(motion.vehiclePositionClear(g, { x: at.x + (p.x - at.x) * i / n, z: at.z + (p.z - at.z) * i / n, rot }, def, block));
        at = p;
      }
    }
    if (checks.clearance) {
      let closest = Infinity, at = from;
      const { x, z, half, minimum } = checks.clearance;
      for (const q of points) {
        for (let k = 0; k <= 40; k++) closest = Math.min(closest, Math.hypot(Math.max(0, Math.abs(at.x + (q.x - at.x) * k / 40 - x) - half), Math.max(0, Math.abs(at.z + (q.z - at.z) * k / 40 - z) - half)));
        at = q;
      }
      assert.ok(points.length && closest >= minimum, `${name}/${type}: clearance ${closest}`);
    }
    if (checks.nonempty) assert.ok(points.length);
    if (checks.mud === 'tank') assert.ok(points.length > 1 && !points.some(p => base.chars[Math.floor(p.z / sim.CELL) * base.w + Math.floor(p.x / sim.CELL)] === 'M'));
    if (checks.mud === 'rifle') assert.equal(points.length, 1);
    if (checks.road) {
      const count = points.filter(p => Math.floor(p.z / sim.CELL) >= 1 && Math.floor(p.z / sim.CELL) <= 3).length;
      if (checks.road === 'tank') assert.ok(count > 5); else assert.equal(count, 0);
    }
    if (checks.large) {
      assert.ok(points.length && Math.hypot(points.at(-1).x - from.x, points.at(-1).z - from.z) < 120);
      assert.ok(expected.expansions <= 4096);
    }
    const query = { type, from, to, terrain, expected, checks };
    if (checks.differentFrom !== undefined) assert.notDeepEqual(coordinates, entry.queries[checks.differentFrom].expected.points);
    entry.queries.push(query);
    return points;
  } };
}
function performanceScene(name, map) {
  const created = create(map, { weather: false, supply: false }, 'buildings');
  created.g.navigationObserved = true;
  created.g.worldNearWalls = sim.__nearWalls({ ...created.g, navigationObserved: false, worldKnown: false });
  return scene(name, created);
}
const W = 40, H = 30;
const mapWith = (fill = '.') => ({ name: 'Path performance regression fixture', w: W, h: H, rows: Array(H).fill(fill.repeat(W)), spawns: [{ x: 2, y: 15 }, { x: 37, y: 15 }], points: [], naval: false });
const rifle = (g, x = 3, y = 15, type = 'rifle') => {
  const u = [...g.units.values()].find(u => u.owner === 0 && u.type === 'rifle');
  assert.ok(u); Object.assign(u, { type, x: (x + .5) * sim.CELL, z: (y + .5) * sim.CELL });
  return u;
};
const destination = { x: 73, z: 31 };
{
  const map = mapWith();
  map.rows = map.rows.map((row, y) => y >= 5 && y < 25 ? row.slice(0, 18) + 'BBBB' + row.slice(22) : row);
  const s = performanceScene('house', map), u = rifle(s.g);
  s.query(u.type, u, destination, { hash: 'house' });
}
{
  const map = mapWith();
  map.heights = Array.from({ length: H }, (_, y) => '0'.repeat(18) + (y === 3 ? '1111' : '2222') + '0'.repeat(18));
  const s = performanceScene('elevation', map), u = rifle(s.g);
  s.query(u.type, u, destination, { hash: 'elevation' });
}
{
  const map = mapWith('W'); map.naval = true;
  map.rows = map.rows.map((row, y) => y >= 12 && y < 18 ? row.slice(0, 18) + '....' + row.slice(22) : row);
  const s = performanceScene('naval', map), u = rifle(s.g, 3, 15, 'gunboat');
  s.query(u.type, u, destination, { hash: 'naval' });
}
{
  const map = { w: 80, h: 80, rows: Array(80).fill('.'.repeat(80)), spawns: [{ x: 20, y: 40 }, { x: 70, y: 40 }], points: [] };
  const s = scene('classic-hq', create(map, { mode: 'classic' }));
  const hq = [...s.g.units.values()].find(u => u.owner === 0 && u.type === 'hq');
  assert.ok(hq?.cells?.length && hq.structureId && s.g.structures.has(hq.structureId));
  assert.ok(hq.cells.every(c => s.g.buildingCells.has(c) && s.g.structuralCells.has(c)));
  for (const type of ['rifle', 'tank']) s.query(type, { owner: 0, x: hq.x - 12, z: hq.z + .3 }, { x: hq.x + 12, z: hq.z }, { clearance: { x: hq.x, z: hq.z, half: sim.UNITS.hq.size * sim.CELL / 2, minimum: 1.9 } });
}
{
  const map = { w: 80, h: 80, rows: Array(80).fill('.'.repeat(80)), spawns: [{ x: 4, y: 4 }, { x: 75, y: 75 }], points: [] };
  const s = scene('annihilation-bunker', create(map, { mode: 'annihilation' }));
  const bunker = [...s.g.units.values()].find(u => u.owner === 0 && u.type === 'bunker');
  Object.assign(bunker, { x: 81, z: 81 }); s.g.players[1].visible.add(bunker.id);
  // Retain the actual authored actor mutation, without introducing a footprint for a cell-free bunker.
  scenes.at(-1).provenance.actors = plain(s.g.units);
  for (const type of ['rifle', 'tank']) {
    const from = { owner: 1, x: 65, z: 81.3 };
    s.query(type, from, { x: 97, z: 81 }, { clearance: { x: 81, z: 81, half: 0, minimum: sim.UNITS.bunker.radius + 1 } });
    s.query(type, from, bunker, { nonempty: true });
  }
}
{
  const s = performanceScene('mines', mapWith()), u = rifle(s.g);
  for (const c of [15 * W + 19, 14 * W + 19, 16 * W + 19, 15 * W + 20]) { s.g.chars[c] = 'N'; s.g.mineSeen.set(c, 1); }
  s.query(u.type, u, destination, { hash: 'mines' });
}
{
  const map = mapWith(); map.rows = map.rows.map(() => '.'.repeat(12) + 'D'.repeat(16) + 'MM' + '.'.repeat(10));
  const s = performanceScene('road-mud', map), u = rifle(s.g, 3, 15, 'tank');
  s.query(u.type, u, destination, { hash: 'roadMud', hull: true });
}
{
  const map = mapWith(), rows = map.rows.map(row => [...row]);
  for (let y = 5; y < 25; y++) if (!(y >= 7 && y <= 9) && !(y >= 21 && y <= 23)) for (let x = 18; x < 22; x++) rows[y][x] = 'B';
  for (let y = 7; y <= 9; y++) for (let x = 12; x <= 27; x++) rows[y][x] = 'D';
  for (let y = 21; y <= 23; y++) for (let x = 12; x <= 27; x++) rows[y][x] = 'M';
  map.rows = rows.map(row => row.join(''));
  const s = performanceScene('wear-wetness', map), u = rifle(s.g, 3, 15, 'tank');
  s.query(u.type, u, destination, { hash: 'freshRoad', hull: true });
  const roadCells = [...s.g.chars.keys()].filter(c => s.g.chars[c] === 'D'), mudCells = [...s.g.chars.keys()].filter(c => s.g.chars[c] === 'M');
  for (const c of roadCells) s.g.wear[c] = 1;
  for (const c of mudCells) s.g.wear[c] = 0;
  s.query(u.type, u, destination, { hash: 'wornRoad', hull: true });
  s.g.wx = { wet: 1 };
  s.query(u.type, u, destination, { differentFrom: 1, hull: true });
  for (const c of roadCells) s.g.wear[c] = 0;
  s.query(u.type, u, destination, { hash: 'freshRoad', hull: true });
}
{
  const s = performanceScene('live-bunker-cache', mapWith()), u = rifle(s.g);
  s.query(u.type, u, destination, { hash: 'bunkerBase' });
  const bunker = { id: 999, type: 'bunker', hp: 3000, x: 41, z: 31, owner: 1 };
  s.g.units.set(999, bunker); s.query(u.type, u, destination, { hash: 'bunkerNear' });
  bunker.z = 7; s.query(u.type, u, destination, { hash: 'bunkerBase' });
  bunker.z = 31; s.query(u.type, u, destination, { hash: 'bunkerNear' });
  bunker.hp = 0; s.query(u.type, u, destination, { hash: 'bunkerBase' });
  bunker.hp = 3000; s.query(u.type, u, destination, { hash: 'bunkerNear' });
  s.g.units.delete(999); s.query(u.type, u, destination, { hash: 'bunkerBase' });
}
const empty = Array(20).fill('.'.repeat(20));
const blank = rows => ({ w: rows[0].length, h: rows.length, rows, spawns: [{ x: 1, y: 1 }, { x: 18, y: 1 }, { x: 1, y: 18 }], points: [{ x: 10, y: 10 }] });
{
  const band = empty.map((row, y) => y >= 7 && y <= 13 ? row.slice(0, 10) + (y === 11 ? '.' : 'M') + row.slice(11) : row);
  const s = scene('mud-selection', create(blank(band), undefined, 'all'));
  s.g.chars.forEach((ch, c) => { if (ch === 'M') s.g.wear[c] = 1; });
  for (const type of ['tank', 'rifle']) s.query(type, { x: 5, z: 21 }, { x: 35, z: 21 }, { mud: type });
}
{
  const rows = empty.map((row, y) => (y >= 1 && y <= 3 ? 'D' : '.').repeat(80));
  const s = scene('road-selection', create(blank(rows), undefined, 'all'));
  for (const type of ['tank', 'rifle']) s.query(type, { x: 3, z: 9 }, { x: 37, z: 9 }, { road: type });
}
{
  const size = 512, N = size * size;
  const g = { w: size, h: size, chars: Array(N).fill('.'), flags: new Uint16Array(N).fill(1024), height: new Int8Array(N), wear: new Float32Array(N),
    worldKnown: true, navigationObserved: true, worldNearWalls: new Uint8Array(N), units: new Map(), mineSeen: new Map(), fires: new Map(), roads: false,
    infantryRegionVersion: 0, vehicleRegionVersion: 0, navalRegionVersion: 0, obstructionVersion: 0, navigationKnowledgeVersion: 0 };
  const s = scene('large512', { g, provenance: { source: 'test-path-performance.mjs:198-213', syntheticOriginal: true } });
  for (const type of ['rifle', 'tank']) for (let n = 0; n < 31; n++) s.query(type, { x: 21, z: 21 }, { x: 995, z: 991 }, { large: true });
}
clean();
const output = JSON.stringify({ reference: pin, node: process.version, arch: process.arch,
  provenance: 'Original test-path-performance.mjs and test.js selected scenes. Seeded startup adapter 0x4d4f5631 uses test-only Mulberry32; unchanged original findPath, navigationLeg, terrain knowledge and hull predicates. Explicit native routing views are component evidence, not native Classic/Annihilation Match admission.',
  oracle, startupDraws, scenes }) + '\n';
const path = `${root}/rust/crates/simtest/tests/fixtures/pathfinding-selected.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), output); else writeFileSync(path, output);
console.log(`${scenes.length} original selected route scenes, ${scenes.reduce((n, s) => n + s.queries.length, 0)} queries`);
