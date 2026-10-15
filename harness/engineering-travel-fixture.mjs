// Original Roads and mud travel cases. No Source instrumentation or body replacement.
// Generation and --check both require an explicit coordinator slot grant.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { gzipSync } from 'node:zlib';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

assert.equal(process.version, 'v24.21.0');
assert.equal(process.arch, 'x64');
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const input = JSON.parse(readFileSync(`${root}/rust/crates/simtest/tests/fixtures/engineering-travel-input.json`, 'utf8'));
assert.equal(input.reference, pin);
assert.deepEqual(input.options, {});
assert.deepEqual([input.warmupTicks, input.measuredTicks], [40, 40]);
assert.deepEqual(input.cases.map(c => [c.name, c.type, c.z, c.surface, c.depth]), [
  ['tank-grass', 'tank', 29, '.', null],
  ['tank-shallow-mud', 'tank', 29, 'M', 0],
  ['tank-deep-mud', 'tank', 29, 'M', 1],
  ['rifle-shallow-mud', 'rifle', 13, 'M', 0],
  ['rifle-grass', 'rifle', 13, '.', null],
]);
const sim = await import(pathToFileURL(`${reference}/shared/sim.js`));
const fixtures = await import(pathToFileURL(`${reference}/test-fixtures.js`));
const bits = n => {
  assert.ok(Number.isFinite(n), `nonfinite captured number ${n}`);
  const b = Buffer.alloc(8);
  b.writeDoubleBE(n);
  return b.toString('hex');
};
const f32bits = n => {
  const b = Buffer.alloc(4);
  b.writeFloatBE(n);
  return b.readUInt32BE();
};
const copy = v => JSON.parse(JSON.stringify(v));
const runs = values => {
  const result = [];
  for (const value of values) {
    if (result.length && Object.is(result.at(-1)[0], value)) result.at(-1)[1]++;
    else result.push([value, 1]);
  }
  return result;
};
function exact(raw) {
  const floatBits = {};
  function visit(value, path = '') {
    if (typeof value === 'number') floatBits[path] = bits(value);
    else if (value && typeof value === 'object') {
      for (const [key, child] of Object.entries(value)) visit(child, path ? `${path}.${key}` : key);
    }
  }
  visit(raw);
  return { raw, floatBits };
}
const point = p => p == null ? null : { x: p.x, z: p.z };
function projection(g, id) {
  assert.equal(g.units.size, 1);
  const u = g.units.get(id);
  assert.ok(u && u.hp > 0 && !u.riding && !u.air);
  return {
    tick: g.tick,
    actor: {
      id, type: u.type, owner: u.owner, x: u.x, z: u.z, rot: u.rot, hp: u.hp,
      supp: u.supp, moveSpeed: u.moveSpeed ?? 0, travelDir: u.travelDir ?? null,
      vx: u.vx ?? 0, vz: u.vz ?? 0, drift: !!u.drift,
      path: u.path.map(point), worldGoal: point(u.worldGoal),
      still: u.still ?? null, repath: u.repath, stuck: u.stuck,
      dust: u.dust ?? null, sprint: u.sprint, holdPos: !!u.holdPos,
      autoRetreat: u.autoRetreat, holdFire: !!u.holdFire, auto: u.auto,
    },
    weather: { now: g.weather.now, next: g.weather.next ?? null, at: g.weather.at ?? 0 },
    wx: copy(g.wx), seed: g.seed >>> 0, wind: copy(g.wind),
    terrain: {
      chars: g.chars.join(''), ground: g.ground.join(''), objects: g.objects.join(''),
      flags: runs(g.flags), height: g.height && runs(g.height),
      wear32: runs([...g.wear].map(f32bits)), hp32: runs([...g.cellHp].map(f32bits)),
      state: runs(g.cellState),
    },
  };
}
function run(spec) {
  const map = { ...copy(input.map), rows: Array(input.map.h).fill(spec.surface.repeat(input.map.w)) };
  const originalRandom = Math.random;
  let startupDraws = 0, g;
  Math.random = () => { startupDraws++; return input.startupRandom; };
  try { g = sim.createGame(map, input.names, input.shuffle); }
  finally { Math.random = originalRandom; }
  const removed = [...g.units.values()].map(u => ({ id: u.id, type: u.type, cells: [...(u.cells ?? [])], structureId: u.structureId ?? null }));
  // These are the original fresh/put/travel setup operations in their original order.
  fixtures.clearFixtureUnits(g);
  g.players.forEach(p => { p.spawn = copy(input.spawnSentinel); });
  g.players[input.owner].mp = input.mpBeforeBuy;
  if (spec.depth !== null) g.wear.fill(spec.depth);
  const purchase = fixtures.fixtureCommand(g, input.owner, { t: 'buy', unit: spec.type });
  assert.equal(purchase, undefined, `${spec.name}: original fixture buy must succeed`);
  const u = [...g.units.values()].at(-1);
  assert.equal(u.type, spec.type);
  Object.assign(u, { x: input.x, z: spec.z, rot: 0, aim: 0, autoRetreat: false });
  const setup = {
    id: u.id, nextId: g.nextId, removed, startupDraws,
    nativeMode: 'Default', teams: g.players.map(p => p.team), factions: g.players.map(p => p.faction),
    seed: g.seed >>> 0, wind: copy(g.wind), wx: copy(g.wx), weather: copy(g.weather),
    players: g.players.map(p => ({ mp: p.mp, spawn: copy(p.spawn) })),
    beforeMove: exact(projection(g, u.id)),
  };
  assert.ok(g.wx && !g.wx.raining && g.wx.rain === 0 && g.wx.wet === 0 && g.wx.next > 4);
  const request = { t: 'move', orders: [[u.id, input.targetX, spec.z]] };
  assert.equal(fixtures.fixtureCommand(g, input.owner, request), undefined);
  const frames = [exact(projection(g, u.id))];
  let before;
  Math.random = () => { throw new Error(`${spec.name}: unexpected global random draw during travel`); };
  try {
    for (let tick = 1; tick <= input.warmupTicks + input.measuredTicks; tick++) {
      sim.step(g);
      assert.equal(g.tick, tick);
      assert.equal(g.wx.rain, 0);
      assert.equal(g.wx.wet, 0);
      frames.push(exact(projection(g, u.id)));
      if (tick === input.warmupTicks) before = u.x;
    }
  } finally { Math.random = originalRandom; }
  assert.equal(g.tick, 80);
  assert.equal(g.wx.rain, 0);
  assert.equal(g.wx.wet, 0);
  return { name: spec.name, map, setup, request, frames, travel: u.x - before };
}
const traces = input.cases.map(run);
const distances = Object.fromEntries(traces.map(t => [t.name, t.travel]));
assert.ok(distances['tank-grass'] > 0);
assert.ok(Math.abs(distances['tank-shallow-mud'] / distances['tank-grass'] - sim.CFG.mudSpeed[0]) < 0.1, 'a tank in shallow mud');
assert.ok(Math.abs(distances['tank-deep-mud'] / distances['tank-grass'] - sim.CFG.mudSpeed[1]) < 0.1, 'a tank in deep mud');
assert.ok(Math.abs(distances['rifle-shallow-mud'] - distances['rifle-grass']) < 0.2, 'infantry walk through mud at full speed');
assert.equal(sim.CFG.mudSpeed[0], input.predicates.tankShallowRatio);
assert.equal(sim.CFG.mudSpeed[1], input.predicates.tankDeepRatio);
clean();
const output = gzipSync(JSON.stringify({ reference: pin, node: process.version, arch: process.arch, input, traces }) + '\n', { level: 9 });
const destination = `${root}/rust/crates/simtest/tests/fixtures/engineering-travel.json.gz`;
if (process.argv.includes('--check')) assert.deepEqual(readFileSync(destination), output);
else writeFileSync(destination, output);
console.log('5 original travel scenes, 400 full Source ticks, 405 frames, 3 selected original assertions');
