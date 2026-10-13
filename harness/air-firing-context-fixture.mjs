// Record unchanged whole-step handling of legitimate aircraft poses outside the map.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
assert.equal(git(['rev-parse', 'HEAD']), pin);
assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');

registerHooks({ load(url, context, next) {
  const result = next(url, context);
  if (url !== pathToFileURL(resolve(reference, 'shared/sim.js')).href) return result;
  return { ...result, source: String(result.source) + '\nexport {airBase,sendPlane,placeUnit,rebuildGrid};\n' };
} });
const sim = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const view = new DataView(new ArrayBuffer(8));
const bits = n => {
  view.setFloat64(0, n);
  return view.getBigUint64(0).toString(16).padStart(16, '0');
};
const map = {
  name: 'Aircraft firing-context arena', w: 40, h: 40,
  rows: Array(40).fill('.'.repeat(40)),
  spawns: [{ x: 3, y: 3 }, { x: 36, y: 36 }],
  points: [{ id: 'front', x: 20, y: 20 }],
};
const actor = u => ({
  id: u.id, kind: u.type, owner: u.owner, x: bits(u.x), z: bits(u.z),
  rot: bits(u.rot), hp: bits(u.hp), air: u.air?.state ?? null,
  holdFire: !!u.holdFire, cooldown: bits(u.cooldown), retarget: bits(u.retarget),
  targetId: u.targetId,
  aviation: u.air ? {
    fuel: bits(u.air.fuel), ammo: bits(u.air.ammo), timer: bits(u.air.timer),
    ang: bits(u.air.ang), mission: u.air.mission ? {
      kind: u.air.mission.kind, id: u.air.mission.id ?? 0,
      x: bits(u.air.mission.x), z: bits(u.air.mission.z),
    } : null,
  } : null,
});
const frame = g => ({
  tick: g.tick, actors: [...g.units.values()].map(actor),
  flights: g.flights.size, shots: structuredClone(g.shots),
});
const cases = [];
for (const kind of ['fighter', 'attacker', 'bomber']) {
  for (const state of ['base', 'rearm', 'out', 'home']) {
    const oldRandom = Math.random;
    let draws = 0;
    Math.random = () => { draws++; return 0.5; };
    try {
      const g = sim.createGame(map, ['A', 'B'], false, [0, 1], [0, 1],
        { weather: false, logistics: false, tech: false });
      g.units.clear(); g.nextId = 1000; sim.rebuildGrid(g);
      const owner = state === 'rearm' || state === 'home' ? 1 : 0;
      const plane = sim.internals.spawnUnit(g, owner, kind);
      plane.holdFire = true;
      const ground = sim.internals.spawnUnit(g, 1 - owner, 'rifle');
      ground.holdFire = true; ground.auto = false;
      const base = sim.airBase(g, plane);
      assert.ok(base.x < 0 || base.z < 0 || base.x >= g.w * sim.CELL || base.z >= g.h * sim.CELL);
      if (state === 'rearm') Object.assign(plane.air, { state, timer: 2 });
      if (state === 'out') sim.sendPlane(plane, { kind: 'patrol', id: 0, x: 40, z: 40 });
      if (state === 'home') {
        // A returning plane can approach its original off-map base from farther outside.
        sim.placeUnit(g, plane, { x: base.x - 12, z: base.z - 12 });
        Object.assign(plane.air, { state, mission: { kind: 'patrol', id: 0, x: 40, z: 40 } });
      }
      assert.equal(plane.air.state, state);
      assert.ok(plane.x < 0 || plane.z < 0 || plane.x >= g.w * sim.CELL || plane.z >= g.h * sim.CELL);
      const before = frame(g), priorDraws = draws;
      sim.step(g);
      const after = frame(g);
      assert.equal(after.tick, before.tick + 1);
      assert.equal(after.actors.length, before.actors.length);
      assert.equal(after.flights, 0);
      assert.deepEqual(after.shots, []);
      assert.equal(after.actors.find(u => u.id === plane.id).hp, before.actors.find(u => u.id === plane.id).hp);
      cases.push({ name: `${kind}-${state}`, kind, state, map,
        base: { x: bits(base.x), z: bits(base.z) },
        before, after, stepRandomDraws: draws - priorDraws });
    } finally { Math.random = oldRandom; }
  }
}
const fixture = { reference: pin, node: process.version, arch: process.arch, cases };
const out = `${root}/rust/crates/simtest/tests/fixtures/air-firing-context.json`;
const bytes = JSON.stringify(fixture, null, 2) + '\n';
if (process.argv.includes('--check')) assert.equal(readFileSync(out, 'utf8'), bytes);
else writeFileSync(out, bytes);
console.log(`${cases.length} off-map aircraft whole-step cases, no generic flights or shots`);
