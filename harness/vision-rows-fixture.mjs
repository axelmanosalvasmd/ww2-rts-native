// Original Source component observations for CMB-124/125/128/131/132.
// Run only with a Root slot grant: node harness/vision-rows-fixture.mjs [--check].
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { decode, encode } from './state-codec.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = (...args) => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const provenance = { pin, sourceBlob: git('rev-parse', `${pin}:shared/sim.js`), node: process.version, arch: process.arch };
const clean = () => {
  assert.equal(git('rev-parse', 'HEAD'), pin);
  assert.equal(git('status', '--porcelain', '--untracked-files=no'), '');
};
assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert(process.argv.slice(2).every(arg => arg === '--check'));
clean();
const localReference = pathToFileURL(resolve(root, 'reference/ww2-rts') + '/').href;
const actualReference = pathToFileURL(reference + '/').href;
registerHooks({
  resolve(specifier, context, next) {
    const url = new URL(specifier, context.parentURL ?? import.meta.url).href;
    if (url.startsWith(localReference)) return next(actualReference + url.slice(localReference.length), context);
    return next(specifier, context);
  },
  load(url, context, next) {
    const result = next(url, context);
    if (url !== actualReference + 'shared/sim.js') return result;
    // Add access to two private functions; their bodies and consumers stay intact.
    return { ...result, source: String(result.source) + '\nexport { visionOf, visionRange };\n' };
  },
});
const sim = await import(actualReference + 'shared/sim.js');
const { rebuildGrid } = await import(actualReference + 'shared/grid.js');
const { tables } = await import('./tables.mjs');
const { trim } = await import('./vision-fixture.mjs');
const map = {
  name: 'Vision row components', w: 96, h: 64,
  rows: Array(64).fill('.'.repeat(96)),
  spawns: [{ x: 2, y: 2 }, { x: 2, y: 61 }, { x: 93, y: 61 }],
  points: [{ x: 48, y: 58 }],
};
const cell = (x, z) => Math.floor(z / sim.CELL) * map.w + Math.floor(x / sim.CELL);
function create(isolated = true) {
  const random = Math.random;
  Math.random = () => 0.5;
  let g;
  try {
    g = sim.createGame(map, ['A', 'Ally', 'Enemy'], false, [0, 0, 1], [0, 1, 2],
      { mode: 'conquest', weather: 'clear', tech: true, logistics: false });
  } finally { Math.random = random; }
  if (isolated) { g.units.clear(); g.nextId = 1; }
  g.tick = 101;
  return g;
}
function spawn(g, owner, type, x = 41, z = 41, fields = {}) {
  const u = sim.internals.spawnUnit(g, owner, type);
  Object.assign(u, { x, z, rot: 0, still: 0, shotAt: undefined, dust: undefined }, fields);
  return u;
}
function input(g) {
  const result = trim(g);
  // Caches are observations of prior phases, not replacement input to each phase.
  delete result.fog;
  return encode(result, { runs: true });
}
const bitsBuffer = new DataView(new ArrayBuffer(8));
function bits(n) {
  bitsBuffer.setFloat64(0, n);
  return bitsBuffer.getBigUint64(0).toString(16).padStart(16, '0');
}
const rangeCases = [];
function range(name, type, weather, rain, level = 0, house = null, built = 1) {
  const g = create(), u = spawn(g, 0, type, 41, 41, { built });
  g.weather.now = weather; g.wx.rain = rain;
  g.height ??= new Int8Array(g.w * g.h);
  g.height[cell(u.x, u.z)] = level;
  if (house !== null) { u.garrison = cell(u.x, u.z); g.house[u.garrison] = house; }
  if (type === 'rifle' && level === 0 && house === null) {
    const multiplier = weather === 'fog' ? 0.7 : weather === 'snow' ? 0.9 : 1;
    assert.equal(sim.visionOf(g, u), sim.UNITS.rifle.vision * multiplier, name);
    assert.equal(sim.visionRange(g, u, sim.UNITS[type]), sim.UNITS.rifle.vision * multiplier * (1 - 0.2 * rain), name);
  }
  rangeCases.push({ name, game: input(g), unit: u.id,
    expected: [bits(sim.visionOf(g, u)), bits(sim.visionRange(g, u, sim.UNITS[type]))] });
}
for (const weather of ['clear', 'fog', 'rain', 'snow']) {
  range(weather, 'rifle', weather, weather === 'rain' ? 1 : 0);
  range(`plane-${weather}`, 'fighter', weather, weather === 'rain' ? 1 : 0);
}
range('rain-half', 'rifle', 'rain', 0.5);
range('hill', 'rifle', 'clear', 0, 3);
for (let house = 0; house < sim.CFG.houses.length; house++) range(`garrison-${house}`, 'rifle', 'snow', 0.25, 2, house);
range('tower-unfinished', 'tower', 'fog', 0.5, 1, null, 0.99);
range('tower-complete', 'tower', 'fog', 0.5, 1);

const visibleCases = [];
function visible(name, distance, expected, setup = () => {}, source = 'rifle', target = 'rifle') {
  const g = create(), eye = spawn(g, 0, source), enemy = spawn(g, 2, target, 41 + distance, 41);
  setup(g, eye, enemy);
  const game = input(g);
  rebuildGrid(g); sim.internals.updateVision(g);
  if (expected !== null) assert.equal(g.players[0].visible.has(enemy.id), expected, name);
  assert.deepEqual([...g.players[0].visible], [...g.players[1].visible], name + ' team sharing');
  visibleCases.push({ name, game, expected: g.players.map(p => [...p.visible]), target: enemy.id });
}
const wall = (g, eye) => sim.internals.setCell(g, cell(eye.x + 3, eye.z), 'H');
visible('close-through-wall', 5.99, true, wall);
visible('six-metre-wall', 6, false, wall);
visible('open-range-edge', sim.UNITS.rifle.vision, true);
visible('beyond-range', sim.UNITS.rifle.vision + 0.01, false);
visible('building-circle', 20, true, wall, 'hq');
visible('fresh-dust', sim.UNITS.rifle.vision * 1.2, true, (g, u, t) => { t.dust = g.tick - 19; }, 'rifle', 'tank');
visible('expired-dust', sim.UNITS.rifle.vision * 1.2, false, (g, u, t) => { t.dust = g.tick - 20; }, 'rifle', 'tank');
for (const [name, distance, still, shotAge, expected] of [
  ['camo-near', 11.99, 3, 80, true], ['camo-edge', 12, 3, 80, false],
  ['camo-not-still', 20, 2.99, 80, true], ['camo-recent-shot', 20, 3, 79, true],
]) visible(name, distance, expected, (g, u, t) => { t.still = still; t.shotAt = g.tick - shotAge; }, 'rifle', 'sniper');
for (const [name, distance, expected] of [['air-camo-near', 23.99, true], ['air-camo-edge', 24, false]]) {
  visible(name, distance, expected, (g, u, t) => {
    u.air.state = 'out'; t.still = 3; t.shotAt = g.tick - 80;
  }, 'fighter', 'sniper');
}
visible('parked-plane-no-source', 10, false, (g, u) => { u.air.state = 'base'; }, 'fighter');
visible('carried-squad-hidden', 5, false, (g, u, t) => { t.riding = u.id; });
for (const [name, distance, shotAge, blocked, expected] of [
  ['trench-near', 17.99, 80, false, true], ['trench-edge', 18, 80, false, false],
  ['trench-shot', 22, 79, false, true], ['trench-wall', 10, 80, true, false],
]) visible(name, distance, expected, (g, u, t) => {
  sim.internals.setCell(g, cell(t.x, t.z), 'T'); t.shotAt = g.tick - shotAge;
  if (blocked) wall(g, u);
});
visible('air-sees-quiet-trench', 25, true, (g, u, t) => {
  u.air.state = 'out'; sim.internals.setCell(g, cell(t.x, t.z), 'T'); t.shotAt = g.tick - 80;
}, 'fighter');
for (const [name, owner, live, offset, expected] of [
  ['recon', 0, true, 0, true], ['recon-ally', 1, true, 0, true],
  ['recon-inactive', 0, false, 0, false], ['recon-enemy', 2, true, 0, false],
  ['recon-edge', 0, true, sim.SUPPORT.recon.width / 2, true],
  ['recon-outside', 0, true, sim.SUPPORT.recon.width / 2 + 0.01, false],
]) visible(name, 75, expected, (g, u, t) => {
  g.strikes.push({ kind: 'recon', owner, live, x: t.x, z: t.z + offset, dir: 0 });
});
visible('ship-nearest-hull', sim.UNITS.rifle.vision + sim.UNITS.gunboat.hull - 0.01, true,
  g => { g.naval = true; }, 'rifle', 'gunboat');
visible('ship-beyond-hull', sim.UNITS.rifle.vision + sim.UNITS.gunboat.hull + 0.01, false,
  g => { g.naval = true; }, 'rifle', 'gunboat');
for (const failed of [false, true]) {
  const g = create(), eye = spawn(g, 0, 'rifle');
  const building = sim.internals.placeBuilding(g, 2, 'barracks', cell(65, 39), true);
  assert(building?.cells?.length, 'canonical building placement');
  if (failed) {
    const nearest = sim.internals.aimPoint(g, eye, building);
    const section = g.structuralCells.get(cell(nearest.x, nearest.z));
    assert(section); section.state = 'failed'; section.hp = 0;
  }
  const aim = sim.internals.aimPoint(g, eye, building);
  assert.equal(sim.los(g, eye, building), false, 'centre is behind footprint');
  if (!failed) assert.equal(sim.los(g, eye, aim), true, 'standing aim cell is visible');
  const game = input(g);
  rebuildGrid(g); sim.internals.updateVision(g);
  if (!failed) assert(g.players[0].visible.has(building.id));
  visibleCases.push({ name: failed ? 'building-failed-nearest-cell' : 'building-aim-point', game,
    expected: g.players.map(p => [...p.visible]), target: building.id });
}

const losCases = [];
function sight(name, a, b, expected, setup = () => {}) {
  const g = create(); setup(g, a, b);
  assert.equal(sim.los(g, a, b), expected, name);
  losCases.push({ name, game: input(g), a, b, expected });
}
const a = { x: 41, z: 41 }, b = { x: 65, z: 41 };
sight('open', a, b, true);
sight('skip-start', a, b, true, g => sim.internals.setCell(g, cell(a.x, a.z), 'H'));
sight('skip-end', a, b, true, g => sim.internals.setCell(g, cell(b.x, b.z), 'H'));
for (const ch of ['H', 'B', 'K']) sight(`block-${ch}`, a, b, false, g => sim.internals.setCell(g, cell(51, 41), ch));
for (const count of [3, 4]) sight(`woods-${count}`, a, b, count === 3, g => {
  for (let i = 0; i < count; i++) sim.internals.setCell(g, cell(45 + 2 * i, 41), 'O');
});
sight('vertical', a, { x: 41, z: 65 }, false, g => sim.internals.setCell(g, cell(41, 51), 'H'));
sight('diagonal', a, { x: 65, z: 65 }, false, g => sim.internals.setCell(g, cell(51, 51), 'H'));
// The endpoint shares a grid corner; cells beyond it must not be visited.
sight('border-endpoint', { x: 40, z: 40 }, { x: 46, z: 42 }, true,
  g => sim.internals.setCell(g, cell(47, 43), 'H'));
sight('off-map', a, { x: -5, z: 41 }, false);
for (const length of [14.99, 15]) sight(`smoke-${length}`, a, { x: a.x + length, z: a.z }, length < 15,
  g => g.smokes.push({ x: 48, z: 41, r: 2 }));
sight('smoke-tangent', a, b, true, g => g.smokes.push({ x: 51, z: 43, r: 2 }));
for (const level of [0, 1]) sight(`hill-${level}`, a, b, level === 0, g => {
  g.height ??= new Int8Array(g.w * g.h); g.height[cell(51, 41)] = level;
});
sight('raised-eyes', a, b, true, g => {
  g.height ??= new Int8Array(g.w * g.h);
  g.height[cell(a.x, a.z)] = 1; g.height[cell(b.x, b.z)] = 1; g.height[cell(51, 41)] = 1;
});

const fogGame = create(), fogEye = spawn(fogGame, 0, 'rifle');
spawn(fogGame, 1, 'rifle', 80, 70);
const fogEnemy = spawn(fogGame, 2, 'rifle', 46, 41);
const fogFrames = [];
let visibleBuilding;
function fogFrame(name, mutate = () => {}) {
  mutate(); fogGame.tick += 4;
  const game = input(fogGame);
  rebuildGrid(fogGame); sim.internals.updateVision(fogGame);
  const expected = [0, 1].map(team => {
    const f = sim.teamFog(fogGame, team);
    return { team, vis: [...f.vis], explored: [...f.explored], version: f.version, deltas: [...f.deltas] };
  });
  const cold = decode(game);
  rebuildGrid(cold); sim.internals.updateVision(cold);
  for (const result of expected) assert.deepEqual([...sim.teamFog(cold, result.team).vis], result.vis, name + ' Source cold mask');
  if (name === 'visible-ground-cell') {
    assert(fogGame.players[0].visible.has(fogEnemy.id));
    assert(Math.hypot(fogEnemy.x - fogEye.x, fogEnemy.z - fogEye.z) > sim.visionRange(fogGame, fogEye, sim.UNITS.rifle));
    assert.equal(expected[0].vis[cell(fogEnemy.x, fogEnemy.z)], 1);
  }
  if (name === 'visible-building-footprint') {
    assert(fogGame.players[0].visible.has(visibleBuilding.id));
    assert(visibleBuilding.cells.some(c => Math.hypot((c % map.w + 0.5) * sim.CELL - fogEye.x,
      (Math.floor(c / map.w) + 0.5) * sim.CELL - fogEye.z) > sim.visionRange(fogGame, fogEye, sim.UNITS.rifle)));
    assert(visibleBuilding.cells.every(c => expected[0].vis[c] === 1));
  }
  fogFrames.push({ name, game, visible: fogGame.players.map(p => [...p.visible]), expected });
}
fogFrame('cold'); fogFrame('warm'); fogFrame('reuse');
fogFrame('moved', () => { fogEye.x += 6; });
fogFrame('fog-range', () => { fogGame.weather.now = 'fog'; });
fogFrame('rain-range', () => { fogGame.weather.now = 'rain'; fogGame.wx.rain = 1; });
fogFrame('sight-change', () => sim.internals.setCell(fogGame, cell(53, 41), 'H'));
fogFrame('height-change', () => {
  fogGame.height ??= new Int8Array(fogGame.w * fogGame.h);
  fogGame.height[cell(55, 41)] = 2; fogGame.terrainVersion++;
});
fogFrame('woods-no-sight-block', () => {
  for (let i = 0; i < 4; i++) sim.internals.setCell(fogGame, cell(47, 47 + 2 * i), 'O');
});
fogFrame('smoke-enter', () => { fogGame.smokes.push({ x: 60, z: 41, r: 4 }); });
fogFrame('smoke-leave', () => { fogGame.smokes.length = 0; });
fogFrame('recon-strip', () => { fogGame.strikes.push({ kind: 'recon', owner: 0, live: true, x: 120, z: 70, dir: 0.5 }); });
fogFrame('recon-ended', () => { fogGame.strikes[0].live = false; });
fogFrame('visible-ground-cell', () => { fogEnemy.x = 77; fogEnemy.z = 55; fogEnemy.dust = fogGame.tick + 4; });
fogFrame('visible-building-footprint', () => { visibleBuilding = sim.internals.placeBuilding(fogGame, 2, 'barracks', cell(69, 47), true); });
fogFrame('building-circle', () => { spawn(fogGame, 0, 'hq', 115, 41); });
fogFrame('airborne-circle', () => { spawn(fogGame, 0, 'fighter', 145, 75, { air: { state: 'out' } }); });
fogFrame('parked-plane', () => { [...fogGame.units.values()].find(u => u.type === 'fighter').air.state = 'base'; });
fogFrame('garrison-range', () => { fogEye.garrison = cell(fogEye.x, fogEye.z); fogGame.house[fogEye.garrison] = 3; });
fogFrame('source-removed', () => { fogGame.units.delete(fogEye.id); });

// CMB-132 uses actual recipient delivery and its existing native initializer.
const wireGame = create(false);
const keys = ['id','type','owner','x','z','rot','aim','hp','supp','cd','cooldown','targetId','still','shotAt','dust','buff','ap','reinf','healAt','retreating','holdFire','holdPos','autoRetreat','auto','xp','built','moveSpeed','vx','vz','travelDir','cargo','riding','garrison','amove','path','worldGoal','attackId','board','fireAt','nade','enter','orders','queue','productionJobs','prog','rally'];
const initial = { map, tick: wireGame.tick, chars: wireGame.chars.join(''),
  inputs: [...wireGame.units.values()].map(u => Object.fromEntries(keys.filter(k => u[k] !== undefined).map(k => [k, structuredClone(u[k])]))),
  players: wireGame.players.map(p => ({ team: p.team, visible: [...p.visible], mp: p.mp, inc: p.inc, vp: p.vp, rally: p.rally })) };
const walker = [...wireGame.units.values()].find(u => u.owner === 0 && !sim.UNITS[u.type].structure);
const wireActions = [
  { name: 'full-start', full: [true, true, true] }, { name: 'unchanged' },
  { name: 'vision', tick: 105, vision: true },
  { name: 'move-delayed-ally', tick: 109, move: { id: walker.id, x: 81, z: 41 }, vision: true, omit: [false, true, false] },
  { name: 'second-move-delayed-ally', tick: 113, move: { id: walker.id, x: 101, z: 61 }, vision: true, omit: [false, true, false] },
  { name: 'catch-up-ally', tick: 117, vision: true },
  { name: 'reconnect', full: [true, false, false] }, { name: 'after-reconnect' },
];
for (const action of wireActions) {
  if (action.tick !== undefined) wireGame.tick = action.tick;
  if (action.move) Object.assign(wireGame.units.get(action.move.id), action.move);
  if (action.vision) { rebuildGrid(wireGame); sim.internals.updateVision(wireGame); }
  action.outputs = wireGame.players.map((p, slot) => {
    const result = action.omit?.[slot] ? undefined : sim.fogFor(wireGame, slot, action.full?.[slot] ?? false);
    return { ...(result === undefined ? {} : { fog: result }), version: p.fog?.version ?? 0, base: [...(p.fog?.base ?? new Uint8Array(map.w * map.h))] };
  });
}
const packCases = [];
for (const size of [0, 1, 31, 32, 33, 1023, 1024, 1025]) for (const pattern of ['zero', 'one', 'edges', 'alternating']) {
  const mask = Uint8Array.from({ length: size }, (_, i) => Number(pattern === 'one' || pattern === 'edges' && (i === 0 || i === size - 1) || pattern === 'alternating' && i % 2 === 0));
  const base = Uint8Array.from({ length: size }, (_, i) => Number(i % 3 === 0));
  for (const previous of [null, base]) packCases.push({ name: `${size}-${pattern}-${previous ? 'base' : 'full'}`, mask: [...mask], base: previous && [...previous], expected: sim.packRuns(mask, previous) });
}
clean();
const files = [
  ['rust/crates/simtest/tests/fixtures/vision-rows.json', { ...provenance, tables: tables(), rangeCases, visibleCases, losCases, fogFrames }],
  ['rust/crates/core/tests/fixtures/vision-wire-batch.json', { ...provenance, initial, actions: wireActions, packCases }],
];
for (const [name, data] of files) {
  const path = resolve(root, name), content = JSON.stringify(data) + '\n';
  if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), content, name);
  else writeFileSync(path, content);
}
console.log(`${rangeCases.length} ranges, ${visibleCases.length} visibility, ${losCases.length} LOS, ${fogFrames.length} fog frames, ${wireActions.length} delivery frames, ${packCases.length} pack cases`);
