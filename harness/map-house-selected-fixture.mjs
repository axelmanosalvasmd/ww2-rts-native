// Record the complete original House types and Map data component checks.
// Run with x64 Node v24.21.0. --check compares without rewriting the fixture.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? resolve(root, 'reference/ww2-rts'));
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
function clean() {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
}
clean();
const sim = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const { clearFixtureUnits, fixtureCommand } = await import(pathToFileURL(resolve(reference, 'test-fixtures.js')));
const bitsBuffer = new ArrayBuffer(8), bitsView = new DataView(bitsBuffer);
const bits = x => { bitsView.setFloat64(0, x); return bitsView.getBigUint64(0).toString(16).padStart(16, '0'); };
const empty = Array(20).fill('.'.repeat(20));
// The original blank helper omits name. Both readers receive this display name.
const blank = rows => ({ name: 't', w: rows[0].length, h: rows.length, rows,
  spawns: [{ x: 1, y: 1 }, { x: 18, y: 1 }, { x: 1, y: 18 }], points: [{ x: 10, y: 10 }] });
function create(map, fresh) {
  assert.equal(sim.validateMap(map), null);
  const g = sim.createGame(map, ['a', 'b'], false);
  clearFixtureUnits(g);
  if (fresh) g.players.forEach(p => { p.spawn = { x: -1000, z: -1000 }; });
  return g;
}
function put(g, x, z, cell) {
  fixtureCommand(g, 0, { t: 'buy', unit: 'rifle' });
  const u = [...g.units.values()].at(-1);
  assert.ok(u && u.owner === 0 && u.type === 'rifle');
  Object.assign(u, { x, z, rot: 0, aim: 0, autoRetreat: false, garrison: cell });
  const row = sim.snapshotFor(g, 0, []).units.find(r => r[0] === u.id);
  assert.ok(row);
  return { input: { id: u.id, type: u.type, owner: u.owner, x, z, rot: u.rot, aim: u.aim,
    autoRetreat: u.autoRetreat, garrison: u.garrison }, row, kind: row[12] >> 20 & 7 };
}
function cells(g, indices) {
  return indices.map(c => {
    const s = g.structuralCells.get(c);
    assert.ok(s && g.structures.get(s.structureId)?.sections.includes(s));
    return { c, char: g.chars[c], house: g.house[c], hp: bits(g.cellHp[c]), tough: g.tough?.[c] ?? 1,
      section: [s.id, s.structureId, s.material, s.anchor, s.supports, s.state, s.rubble, bits(s.hp), bits(s.maxHp)] };
  });
}
const originalRandom = Math.random;
let draws = 0;
Math.random = () => { draws++; return 0.5; };
try {
  const cases = [];
  {
    const rows = [...empty]; rows[1] = 'B' + '.'.repeat(19);
    for (const y of [4, 5, 6]) rows[y] = 'BBBB' + '.'.repeat(16);
    for (const y of [10, 11, 12, 13]) rows[y] = '.'.repeat(8) + 'BBBBBB' + '.'.repeat(6);
    const map = blank(rows), g = create(map, true), indices = [20, 80, 208];
    const initial = cells(g, indices);
    assert.deepEqual(indices.map(c => g.house[c]), [0, 1, 2]);
    assert.deepEqual(indices.map(c => g.cellHp[c]), sim.CFG.houses.slice(0, 3).map(h => h.hp));
    const full = indices.map(c => sim.garrisonMul(g, c));
    assert.deepEqual(full, sim.CFG.houses.slice(0, 3).map(h => h.mul));
    g.cellHp[80] /= 2;
    const half = sim.garrisonMul(g, 80);
    assert.ok(Math.abs(half - (sim.CFG.houses[1].mul + sim.CFG.houses[1].worn) / 2) < 1e-6);
    const actor = put(g, 3, 21, 208);
    assert.equal(actor.kind, 2);
    cases.push({ name: 'House types', source: 'test.js:773-784', map, initial,
      full: full.map(bits), half: bits(half), halfHp: bits(g.cellHp[80]), actor });
  }
  {
    const rows = [...empty]; rows[1] = 'B' + '.'.repeat(19);
    rows[6] = '.'.repeat(5) + 'W'.repeat(10) + '.'.repeat(5);
    rows[7] = '.'.repeat(5) + 'W'.repeat(4) + '==' + 'W'.repeat(4) + '.'.repeat(5);
    rows[8] = '.'.repeat(5) + 'W'.repeat(10) + '.'.repeat(5);
    const map = { ...blank(rows), buildings: [{ x: 0, y: 1, kind: 'church' }, { x: 10, y: 7, kind: 'stone bridge' }] };
    const g = create(map, false), initial = cells(g, [20, 149, 150]);
    assert.equal(g.house[20], 3);
    assert.equal(g.cellHp[20], sim.CFG.houses[3].hp);
    const actor = put(g, 1, 3, 20);
    assert.equal(actor.kind, 3);
    for (const c of [149, 150]) assert.equal(g.cellHp[c], sim.CFG.terrainHp['='] * sim.CFG.stoneBridge);
    const blast = { x: 19, z: 15, radius: 1, damage: sim.SUPPORT.dive.terrain, scar: 0, actors: [] };
    const context = { tick: g.tick, seed: g.seed >>> 0, nextId: g.nextId, rain: g.wx?.rain ?? 0,
      wet: g.wx?.wet ?? 0, windAngle: g.wind.a, windStrength: g.wind.v };
    sim.damageCells(g, [], { x: blast.x, z: blast.z }, blast.radius, blast.damage);
    assert.equal(g.chars[149], '=');
    const invalid = [[{ x: 3, y: 3, kind: 'church' }], [{ x: 0, y: 1, kind: 'castle' }]].map(buildings => {
      const input = { ...map, buildings }, error = sim.validateMap(input);
      assert.ok(error);
      return { map: input, error };
    });
    cases.push({ name: 'Map data', source: 'test.js:788-807', map, initial, actor, blast, context,
      after: cells(g, [149, 150]), invalid });
  }
  clean();
  const output = JSON.stringify({ schemaVersion: 1, reference: pin, node: process.version, arch: process.arch,
    initializationRandom: { constant: 0.5, draws }, cases }) + '\n';
  const target = resolve(root, 'rust/crates/core/tests/fixtures/map-house-selected.json');
  if (process.argv.includes('--check')) assert.equal(readFileSync(target, 'utf8'), output);
  else writeFileSync(target, output);
  console.log(`${process.argv.includes('--check') ? 'Verified' : 'Recorded'} 2 complete original named house checks`);
} finally {
  Math.random = originalRandom;
  clean();
}
